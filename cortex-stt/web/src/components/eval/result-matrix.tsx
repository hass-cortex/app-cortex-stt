import { AlertTriangle } from "lucide-react";
import { type ReactNode, useCallback, useMemo, useState } from "react";
import { evalAudioUrl } from "@/api/client";
import type { EvalResult, EvalSample } from "@/api/types";
import { PlayButton } from "@/components/eval/play-button";
import { ResultGrid } from "@/components/eval/result-grid";
import { Hint } from "@/components/ui/hint";
import { Segmented } from "@/components/ui/segmented";
import { useJudge } from "@/hooks/use-eval";
import { diffCharacters, withoutTrailingPunctuation } from "@/lib/transcript";
import { verdictOf } from "@/lib/verdict";

interface ResultMatrixProps {
	samples: EvalSample[];
	results: EvalResult[];
	onSelectSample?: (sampleId: string) => void;
	/** What is narrowing this table, and how to release it. Rendered on
	 *  the table's own toolbar, beside the controls that shape it. */
	filterNotice?: ReactNode;
}

type View = "detail" | "grid";

const VIEWS: { value: View; label: string }[] = [
	{ value: "detail", label: "Transcripts" },
	{ value: "grid", label: "Grid" },
];

/**
 * Reference transcript pinned on the left, one column per candidate.
 *
 * Every cell carries a verdict: the server's comparison decides the ones
 * nobody has ruled on, and a person's mark overrides it in either
 * direction and outlives the run. What the mark is *not* is a character
 * error rate — without script normalisation a perfectly correct
 * transcript differs from a traditional-character reference at nearly
 * every position, so a distance here would be noise.
 */
export function ResultMatrix({
	samples,
	results,
	onSelectSample,
	filterNotice,
}: ResultMatrixProps) {
	// Two readings of one table: what each model heard, and who failed
	// where. Neither is a summary of the other, so it is a switch rather
	// than a detail level.
	const [view, setView] = useState<View>("detail");
	// Off by default: the marks answer "where did this one go wrong", which
	// is a question you ask of a row, not of the whole table at a glance.
	const [showDiff, setShowDiff] = useState(false);
	const judge = useJudge();

	const models = useMemo(() => [...new Set(results.map((r) => r.model_id))].sort(), [results]);

	const byCell = useMemo(() => {
		const map = new Map<string, EvalResult>();
		for (const r of results) map.set(`${r.sample_id}\u0000${r.model_id}`, r);
		return map;
	}, [results]);

	const rows = samples.filter((s) => results.some((r) => r.sample_id === s.id));

	const resultOf = useCallback(
		(sampleId: string, modelId: string) => byCell.get(`${sampleId}\u0000${modelId}`),
		[byCell],
	);

	if (rows.length === 0) {
		return <p className="text-sm text-text-muted">This run has no results yet.</p>;
	}

	return (
		<>
			<div className="flex flex-wrap justify-end items-center gap-2 pb-3">
				{filterNotice && <div className="mr-auto">{filterNotice}</div>}
				{view === "detail" && (
					<button
						type="button"
						onClick={() => setShowDiff((on) => !on)}
						aria-pressed={showDiff}
						className={`num px-2.5 py-[5px] rounded-[7px] border text-[11.5px] transition-colors cursor-pointer ${
							showDiff
								? "bg-accent-wash border-accent-quiet text-text-primary"
								: "bg-surface-3 border-border text-text-muted hover:text-text-primary"
						}`}
					>
						Mark differences
					</button>
				)}
				<Segmented options={VIEWS} value={view} onChange={setView} />
			</div>

			{view === "grid" ? (
				<ResultGrid
					samples={rows}
					models={models}
					resultOf={resultOf}
					onSelectSample={onSelectSample}
				/>
			) : (
				<div className="overflow-x-auto">
					<table className="w-full min-w-max text-sm border-collapse">
						<thead>
							<tr className="text-left">
								{/* Pinned where there is width to spare, so scrolling to a
								    far model keeps the row it belongs to in view. On a phone
								    it would hold most of the screen, so the table scrolls as
								    one piece instead — same rule as the Grid. */}
								<th className="sm:sticky sm:left-0 sm:z-10 bg-surface-2 py-2 pr-4 text-xs font-medium uppercase tracking-wider text-text-muted whitespace-nowrap">
									Reference
								</th>
								{models.map((m) => (
									<th
										key={m}
										className="py-2 pr-4 text-xs font-medium uppercase tracking-wider text-text-muted whitespace-nowrap"
									>
										{m}
									</th>
								))}
							</tr>
						</thead>
						<tbody>
							{rows.map((sample) => (
								<tr key={sample.id} className="border-t border-border align-top">
									<td className="sm:sticky sm:left-0 sm:z-10 bg-surface-2 py-2 pr-4 border-r border-border">
										<div className="flex items-start gap-1.5">
											<PlayButton src={evalAudioUrl(sample.id)} />
											<button
												type="button"
												onClick={() => onSelectSample?.(sample.id)}
												className="text-left font-medium text-text-primary hover:text-accent cursor-pointer"
											>
												{sample.reference_transcript}
											</button>
										</div>
									</td>
									{models.map((model) => {
										const cell = byCell.get(`${sample.id}\u0000${model}`);
										const durationMs = sample.audio_duration_ms;
										if (!cell) {
											return (
												<td key={model} className="py-2 pr-4 text-text-muted">
													—
												</td>
											);
										}
										const { correct, confirmed } = verdictOf(cell);
										return (
											// The verdict as a wash over the whole cell: a tick is a glyph
											// you read, a colour is a shape you scan. A correct nobody has
											// checked washes fainter — that is the one that adds a point on
											// the comparison's word alone.
											<td
												key={model}
												className={`py-2 pr-4 pl-2 ${
													correct
														? confirmed
															? "bg-success/10"
															: "bg-success/[0.04]"
														: "bg-error/10"
												}`}
											>
												<div className="flex items-start gap-2">
													<span className="min-w-0">
														<span
															className={
																cell.error_message
																	? "text-error text-xs"
																	: "text-text-primary break-words"
															}
														>
															{cell.error_message ??
																(showDiff ? (
																	<Diff
																		reference={sample.reference_transcript}
																		output={cell.text}
																		matched={cell.matches_reference}
																	/>
																) : (
																	cell.text || <span className="text-text-muted">(empty)</span>
																))}
														</span>
														{/* Timing is only comparable within one run, which is
												    exactly the comparison this table is making. */}
														{!cell.error_message && (
															<span className="block text-xs text-text-muted font-mono tabular-nums">
																{cell.inference_ms} ms
																{durationMs > 0 &&
																	` · ${(cell.inference_ms / durationMs).toFixed(2)}×`}
																{cell.raw_text && (
																	<Hint
																		label="What the model wrote before conversion"
																		content={cell.raw_text}
																		className="ml-1.5 align-[-1px] text-warning/80"
																	>
																		<AlertTriangle size={10} strokeWidth={2} />
																	</Hint>
																)}
															</span>
														)}
													</span>
													{/* Pressed means "a person said so". Clicking the side the
													    comparison already picked still records a person's
													    agreement — the difference between a figure that was
													    checked and one that was not; clicking it again withdraws
													    that and hands the cell back to the rule. */}
													{!cell.error_message && (
														<span className="flex gap-0.5 shrink-0">
															<VerdictButton
																active={confirmed && correct}
																label="Correct"
																symbol="✓"
																tone="text-success"
																onClick={() =>
																	judge.mutate({
																		sampleId: sample.id,
																		outputText: cell.text,
																		correct: cell.ruling === true ? null : true,
																	})
																}
															/>
															<VerdictButton
																active={confirmed && !correct}
																label="Wrong"
																symbol="✗"
																tone="text-error"
																onClick={() =>
																	judge.mutate({
																		sampleId: sample.id,
																		outputText: cell.text,
																		correct: cell.ruling === false ? null : false,
																	})
																}
															/>
														</span>
													)}
												</div>
											</td>
										);
									})}
								</tr>
							))}
						</tbody>
					</table>
				</div>
			)}

			{view === "detail" && (showDiff || results.some((r) => r.raw_text)) && (
				<div className="flex flex-wrap items-center gap-x-4 gap-y-1 pt-2 text-xs text-text-muted">
					{showDiff && (
						<>
							<span>
								<span className="bg-error-wash text-error rounded-[2px] px-[3px]">red</span> what
								the model put there
							</span>
							<span>
								<span className="text-text-faint line-through">struck through</span> what it missed
							</span>
						</>
					)}
					{results.some((r) => r.raw_text) && (
						<span className="flex items-center gap-1.5">
							<AlertTriangle size={11} strokeWidth={2} className="text-warning/80 shrink-0" />
							the model wrote it in another script and the conversion rendered it — tap the mark to
							read what it wrote
						</span>
					)}
				</div>
			)}
		</>
	);
}

/**
 * The output, marked against its reference.
 *
 * An output the comparison already accepts is shown plain: it differs
 * only in what the comparison folds away, and colouring that would make
 * every correct row look wrong.
 */
function Diff({
	reference,
	output,
	matched,
}: {
	reference: string;
	output: string;
	matched: boolean;
}) {
	if (!output) return <span className="text-text-muted">(empty)</span>;
	if (matched) return <>{output}</>;

	// Both ends stripped before diffing: a mark at the end is not a word
	// the model got wrong. It is still printed, just never in red.
	const body = withoutTrailingPunctuation(output);
	const trailing = output.slice(body.length);
	const parts = [
		...diffCharacters(withoutTrailingPunctuation(reference), body),
		...(trailing ? [{ text: trailing, kind: "same" as const }] : []),
	];

	return (
		<>
			{parts.map((part, index) => (
				<span
					// biome-ignore lint/suspicious/noArrayIndexKey: a run's identity IS its position in the string
					key={index}
					className={
						part.kind === "added"
							? "bg-error-wash text-error rounded-[2px]"
							: part.kind === "removed"
								? "text-text-faint line-through"
								: ""
					}
				>
					{part.text}
				</span>
			))}
		</>
	);
}

function VerdictButton({
	active,
	label,
	symbol,
	tone,
	onClick,
}: {
	active: boolean;
	label: string;
	symbol: string;
	tone: string;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			aria-label={label}
			aria-pressed={active}
			onClick={onClick}
			className={`w-5 h-5 rounded text-xs leading-none cursor-pointer transition-colors ${
				active ? `${tone} bg-surface-3` : "text-text-muted hover:bg-surface-3"
			}`}
		>
			{symbol}
		</button>
	);
}
