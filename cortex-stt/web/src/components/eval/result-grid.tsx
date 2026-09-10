import { useCallback, useMemo, useRef, useState } from "react";
import { evalAudioUrl } from "@/api/client";
import type { EvalResult, EvalSample } from "@/api/types";
import { PlayButton } from "@/components/eval/play-button";
import { useDismiss } from "@/hooks/use-dismiss";
import { verdictOf } from "@/lib/verdict";

interface ResultGridProps {
	samples: EvalSample[];
	models: string[];
	resultOf: (sampleId: string, modelId: string) => EvalResult | undefined;
	onSelectSample?: (sampleId: string) => void;
}

/**
 * Verdict by hue — and, for one of them, whose word it is.
 *
 * Only an unchecked *correct* is called out. It is the verdict that adds
 * a point, so an unnoticed mistake in it inflates a model and can decide
 * which one gets promoted; the comparison drops punctuation, so a model
 * that swallowed a `%` lands here looking right. An unchecked *wrong* is
 * already costing the model that point — measured over 212 cells a
 * person had ruled wrong, the comparison was mistaken once — so telling
 * it apart earns nothing and costs a fifth thing to read.
 */
type Mark = "correct" | "correct-unchecked" | "wrong" | "missing";

const markClass: Record<Mark, string> = {
	correct: "bg-success/70",
	"correct-unchecked": "bg-success/25",
	wrong: "bg-error/30",
	missing: "bg-transparent border border-border-soft",
};

const markLabel: Record<Mark, string> = {
	correct: "a person marked this correct",
	"correct-unchecked": "matches the reference, unchecked",
	wrong: "wrong",
	missing: "no result",
};

/** Short enough to sit in a row under the grid. */
const keyLabel: Record<Mark, string> = {
	correct: "correct",
	"correct-unchecked": "correct · unchecked",
	wrong: "wrong",
	missing: "no result",
};

const markTone: Record<Mark, string> = {
	correct: "text-success",
	"correct-unchecked": "text-success/70",
	wrong: "text-error",
	missing: "text-text-faint",
};

interface Hover {
	sample: EvalSample;
	model: string;
	mark: Mark;
	cell?: EvalResult;
	/** Viewport coordinates of the cell, so the card can sit beside it. */
	rect: DOMRect;
	/** The cell's row and column scores — the reason it is in this position. */
	sampleScore: number;
	modelScore: number;
	models: number;
	samples: number;
}

/**
 * One ruling per cell, nothing else.
 *
 * Sorted hardest sample first and strongest model first, so the boundary
 * between what every model gets and what none of them do falls on a
 * diagonal — the shape a table of transcripts cannot show. Every cell
 * carries a verdict, so the ordering is over the same figures the
 * summary tables report; the shade says whether a person confirmed it.
 */
export function ResultGrid({ samples, models, resultOf, onSelectSample }: ResultGridProps) {
	// A cell is 20px of colour; what it stands for has to arrive at once.
	// The native `title` waits about a second, which is longer than the
	// pointer spends crossing the row.
	const [hover, setHover] = useState<Hover | null>(null);
	// A touch device never crosses a cell, it lands on one. There the first
	// tap opens the card and the second follows the cell through to the
	// sample, so the card stays reachable without costing the mouse a click.
	const [pinned, setPinned] = useState<string | null>(null);
	const grid = useRef<HTMLDivElement>(null);
	const pointerKind = useRef<string>("mouse");

	const clear = useCallback(() => {
		setPinned(null);
		setHover(null);
	}, []);
	useDismiss(pinned !== null, clear, [grid]);

	const { rows, columns, markOf, correctBySample, correctByModel } = useMemo(() => {
		const mark = (sampleId: string, modelId: string): Mark => {
			const cell = resultOf(sampleId, modelId);
			if (!cell || cell.error_message) return "missing";
			const { correct, confirmed } = verdictOf(cell);
			if (!correct) return "wrong";
			return confirmed ? "correct" : "correct-unchecked";
		};

		// The row and column counts are of the verdict, not of who gave it:
		// a score that only counted confirmed cells would rank a model by
		// how much attention it happened to get.
		const right = (sampleId: string, modelId: string) =>
			mark(sampleId, modelId).startsWith("correct");
		const bySample = new Map(
			samples.map((s) => [s.id, models.filter((m) => right(s.id, m)).length]),
		);
		const byModel = new Map(models.map((m) => [m, samples.filter((s) => right(s.id, m)).length]));

		return {
			markOf: mark,
			correctBySample: bySample,
			correctByModel: byModel,
			rows: [...samples].sort(
				(a, b) => (bySample.get(a.id) ?? 0) - (bySample.get(b.id) ?? 0) || a.id.localeCompare(b.id),
			),
			columns: [...models].sort(
				(a, b) => (byModel.get(b) ?? 0) - (byModel.get(a) ?? 0) || a.localeCompare(b),
			),
		};
	}, [samples, models, resultOf]);

	return (
		<>
			<div ref={grid} className="overflow-x-auto">
				{/* One leave handler for the whole grid: per-cell leave/enter
				    pairs would blink the card off between adjacent squares. */}
				<table
					className="border-separate border-spacing-[2px]"
					onPointerLeave={(e) => {
						if (e.pointerType === "mouse" && !pinned) setHover(null);
					}}
				>
					<thead>
						<tr>
							{/* Pinned only where there is room to spare. On a phone the
							    reference column would hold most of the width while the
							    cells crawl past in what is left, so the grid scrolls as
							    one piece instead. */}
							<th className="sm:sticky sm:left-0 sm:z-10 bg-surface-2 align-bottom" />
							<th className="align-bottom" />
							{columns.map((model) => (
								<th key={model} className="align-bottom p-0">
									{/* No height cap: the label sizes the header, because a
									    clipped model id is not a model id. */}
									<div className="num text-[10px] text-text-muted whitespace-nowrap [writing-mode:vertical-rl] rotate-180 mx-auto">
										{model}
									</div>
								</th>
							))}
						</tr>
					</thead>
					<tbody>
						{rows.map((sample) => (
							<tr key={sample.id}>
								<td className="sm:sticky sm:left-0 sm:z-10 bg-surface-2 pr-3">
									{/* The row label is a truncated line, so the reference is the
									    one thing here that cannot be read in full — hearing the
									    clip is how you check it, and walking to another view to
									    do that is how a wrong reference survives a whole run. */}
									<div className="flex items-center gap-1.5 w-[240px]">
										<PlayButton src={evalAudioUrl(sample.id)} />
										<button
											type="button"
											onClick={() => onSelectSample?.(sample.id)}
											className="flex-1 min-w-0 text-left text-[12.5px] text-text-primary truncate hover:text-accent cursor-pointer"
										>
											{sample.reference_transcript}
										</button>
									</div>
								</td>
								<td className="num pr-2 text-right text-[10.5px] text-text-faint tabular-nums">
									{correctBySample.get(sample.id) ?? 0}
								</td>
								{columns.map((model) => {
									const mark = markOf(sample.id, model);
									const show = (event: { currentTarget: HTMLElement }) =>
										setHover({
											sample,
											model,
											mark,
											cell: resultOf(sample.id, model),
											rect: event.currentTarget.getBoundingClientRect(),
											sampleScore: correctBySample.get(sample.id) ?? 0,
											modelScore: correctByModel.get(model) ?? 0,
											models: columns.length,
											samples: rows.length,
										});
									const key = `${sample.id}\u0000${model}`;
									return (
										<td key={model} className="p-0">
											<button
												type="button"
												onPointerDown={(e) => {
													pointerKind.current = e.pointerType;
												}}
												onClick={(event) => {
													if (pointerKind.current !== "mouse" && pinned !== key) {
														setPinned(key);
														show(event);
														return;
													}
													clear();
													onSelectSample?.(sample.id);
												}}
												onPointerEnter={(event) => {
													if (event.pointerType === "mouse" && !pinned) show(event);
												}}
												onFocus={show}
												onBlur={() => {
													if (!pinned) setHover(null);
												}}
												className={`block w-5 h-5 rounded-[2px] cursor-pointer ${markClass[mark]}`}
												aria-label={`${model}: ${markLabel[mark]}`}
											/>
										</td>
									);
								})}
							</tr>
						))}
					</tbody>
				</table>
			</div>

			{hover && <HoverCard hover={hover} />}

			<div className="flex flex-wrap items-center gap-x-4 gap-y-1 pt-3 text-xs text-text-muted">
				<Key mark="correct" />
				<Key mark="correct-unchecked" />
				<Key mark="wrong" />
				<Key mark="missing" />
				<span className="text-text-faint">
					hardest sample first · strongest model first · the number is how many models got that
					sample
				</span>
			</div>
		</>
	);
}

const CARD_WIDTH = 340;
const CARD_GAP = 10;

/** Beside the cell, never under the pointer: the card is read while the
 *  pointer is still on the square that opened it. */
function HoverCard({ hover }: { hover: Hover }) {
	const { sample, model, mark, cell, rect, sampleScore, modelScore, models, samples } = hover;
	const durationMs = sample.audio_duration_ms;

	// To the right of the cell, flipping left when it would run off: the
	// sample names on the left stay readable while the card is open, which
	// is what tells you which row you are on. On a phone neither side has
	// room, so the card takes the width it can get.
	const width = Math.min(CARD_WIDTH, window.innerWidth - 2 * CARD_GAP);
	const fitsRight = rect.right + CARD_GAP + width <= window.innerWidth;
	const left = fitsRight ? rect.right + CARD_GAP : Math.max(CARD_GAP, rect.left - CARD_GAP - width);
	const top = Math.min(Math.max(CARD_GAP, rect.top - 40), window.innerHeight - 220);

	return (
		<div
			className="fixed z-50 pointer-events-none px-3 py-2.5 bg-surface-1 border border-border rounded-lg shadow-[0_8px_24px_rgba(0,0,0,0.45)]"
			style={{ width, left, top }}
		>
			<div className="flex items-baseline gap-2">
				<span className="num text-[11px] text-text-primary truncate">{model}</span>
				<span className={`num ml-auto text-[10.5px] shrink-0 ${markTone[mark]}`}>
					{markLabel[mark]}
				</span>
			</div>

			<Line label="REFERENCE" value={sample.reference_transcript} />
			<Line
				label="HEARD"
				value={cell?.error_message ?? cell?.text ?? "—"}
				tone={cell?.error_message ? "text-error" : "text-text-primary"}
			/>

			{cell && !cell.error_message && (
				<div className="num mt-2 pt-2 border-t border-border-soft flex flex-wrap gap-x-3 text-[10.5px] text-text-muted">
					<span>
						infer <span className="text-text-primary">{cell.inference_ms} ms</span>
					</span>
					{durationMs > 0 && (
						<span>
							rtf{" "}
							<span className="text-text-primary">
								{(cell.inference_ms / durationMs).toFixed(2)}×
							</span>
						</span>
					)}
					<span>
						audio <span className="text-text-primary">{(durationMs / 1000).toFixed(2)} s</span>
					</span>
					{cell.raw_text && <span className="text-warning">converted from another script</span>}
				</div>
			)}

			<div className="num mt-1.5 flex flex-wrap gap-x-3 text-[10.5px] text-text-faint">
				<span>
					this sample {sampleScore}/{models} models
				</span>
				<span>
					this model {modelScore}/{samples} samples
				</span>
			</div>
		</div>
	);
}

function Line({ label, value, tone }: { label: string; value: string; tone?: string }) {
	return (
		<div className="mt-1.5">
			<div className="num text-[9.5px] tracking-[0.06em] text-text-faint">{label}</div>
			<div className={`text-[12.5px] leading-snug break-words ${tone ?? "text-text-secondary"}`}>
				{value || <span className="text-text-muted italic">empty</span>}
			</div>
		</div>
	);
}

function Key({ mark }: { mark: Mark }) {
	return (
		<span className="inline-flex items-center gap-1.5">
			<span className={`w-3 h-3 rounded-[2px] ${markClass[mark]}`} />
			{keyLabel[mark]}
		</span>
	);
}
