import { useEffect, useMemo, useRef, useState } from "react";
import type { ModelInfo } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { Input } from "@/components/ui/input";
import { Modal } from "@/components/ui/modal";
import { SearchBox } from "@/components/ui/search-box";
import { useToast } from "@/components/ui/toast";
import { useRuns, useSamples, useStartRun } from "@/hooks/use-eval";
import { useModels } from "@/hooks/use-models";
import { formatBytes, formatDuration, formatRelativeTime } from "@/lib/format";

interface NewRunDialogProps {
	open: boolean;
	onClose: () => void;
	sampleCount: number;
	availableMemoryBytes?: number;
	/** Open with only these samples ticked — the Evaluation set hands its
	 *  own selection over rather than making it again here. Null means the
	 *  whole set, which is what the page's own New run button wants. */
	initialSampleIds?: string[] | null;
	onStarted: (runId: string) => void;
}

/**
 * Choosing candidates.
 *
 * Only installed models appear: evaluation reads the installed set and
 * never downloads, so this button cannot quietly pull gigabytes and
 * announce a string of Installs to Home Assistant.
 *
 * Declared languages are a column, not a filter. A model that claims zh
 * and emits Thai has to stay on the table — hiding it would remove the
 * evidence the run exists to produce.
 */
export function NewRunDialog({
	open,
	onClose,
	sampleCount,
	availableMemoryBytes,
	initialSampleIds = null,
	onStarted,
}: NewRunDialogProps) {
	const { data: models = [] } = useModels();
	const { data: runs = [], isLoading: runsLoading } = useRuns();
	const [selected, setSelected] = useState<Set<string>>(new Set());
	const [language, setLanguage] = useState("");
	const { data: samples = [] } = useSamples();
	// Everything, until you say otherwise. Deselecting is the deliberate
	// act, because narrowing the set is what changes what a score means.
	const [excluded, setExcluded] = useState<Set<string>>(new Set());
	const chosen = samples.filter((s) => !excluded.has(s.id));
	const start = useStartRun();
	const { toast } = useToast();

	// Search narrows what is on screen; it never narrows the run. A hidden
	// row keeps whatever it was, so typing in the box cannot silently drop
	// a model or a sample from a run that is about to start — the counts
	// above each list are what say who is in.
	const [modelQuery, setModelQuery] = useState("");
	const [sampleQuery, setSampleQuery] = useState("");

	const installed = useMemo(
		() => models.filter((m: ModelInfo) => m.status === "downloaded" || m.status === "custom"),
		[models],
	);
	// Declared languages match too: "zh" is how you reach the candidates
	// worth comparing on a Chinese clip.
	const shownModels = useMemo(() => {
		const q = modelQuery.trim().toLowerCase();
		if (!q) return installed;
		return installed.filter(
			(m: ModelInfo) =>
				m.id.toLowerCase().includes(q) ||
				m.name.toLowerCase().includes(q) ||
				m.family.toLowerCase().includes(q) ||
				m.languages.some((l) => l.toLowerCase().includes(q)),
		);
	}, [installed, modelQuery]);

	const shownSamples = useMemo(() => {
		const q = sampleQuery.trim().toLowerCase();
		if (!q) return samples;
		return samples.filter(
			(s) =>
				s.reference_transcript.toLowerCase().includes(q) ||
				(s.capture_device ?? "").toLowerCase().includes(q),
		);
	}, [samples, sampleQuery]);

	const previous = runs[0];

	// Both starting states are seeded once per opening, and only once the
	// data they read has arrived: a late fetch must not overwrite a value
	// already changed — or deliberately cleared — in this dialog.
	const seededSamples = useRef(false);
	useEffect(() => {
		if (!open) {
			seededSamples.current = false;
			return;
		}
		if (seededSamples.current || samples.length === 0) return;
		seededSamples.current = true;
		setExcluded(
			initialSampleIds === null
				? new Set()
				: new Set(samples.filter((s) => !initialSampleIds.includes(s.id)).map((s) => s.id)),
		);
	}, [open, samples, initialSampleIds]);

	const seededHint = useRef(false);
	useEffect(() => {
		if (!open) {
			seededHint.current = false;
			return;
		}
		if (seededHint.current || runsLoading) return;
		seededHint.current = true;
		setLanguage(previous?.language ?? "");
	}, [open, runsLoading, previous?.language]);

	const toggle = (id: string) =>
		setSelected((prev) => {
			const next = new Set(prev);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});

	// All/None act on the rows in view, so they mean what they look like
	// while a search is active. Rows filtered out are left alone.
	const selectAllModels = () =>
		setSelected((prev) => new Set([...prev, ...shownModels.map((m: ModelInfo) => m.id)]));
	const selectNoModels = () =>
		setSelected((prev) => {
			const next = new Set(prev);
			for (const m of shownModels) next.delete(m.id);
			return next;
		});
	const includeAllSamples = () =>
		setExcluded((prev) => {
			const next = new Set(prev);
			for (const s of shownSamples) next.delete(s.id);
			return next;
		});
	const excludeAllSamples = () =>
		setExcluded((prev) => new Set([...prev, ...shownSamples.map((s) => s.id)]));

	return (
		<Modal
			open={open}
			onClose={onClose}
			title="New evaluation run"
			width="xl"
			footer={
				<div className="flex justify-end gap-2">
					<Button variant="ghost" onClick={onClose}>
						Cancel
					</Button>
					<Button
						disabled={selected.size === 0 || chosen.length === 0}
						loading={start.isPending}
						onClick={() =>
							start.mutate(
								{
									modelIds: [...selected],
									language: language || undefined,
									// Only send a list when it is a real subset; a run
									// over everything should say so, not enumerate.
									sampleIds: chosen.length === samples.length ? undefined : chosen.map((s) => s.id),
								},
								{
									onSuccess: (data) => {
										toast("Run started", "success");
										onStarted(data.run_id);
										onClose();
									},
									onError: (err) => toast(`Could not start the run: ${err.message}`, "error"),
								},
							)
						}
					>
						Start
					</Button>
				</div>
			}
		>
			<div className="space-y-4">
				<div className="flex flex-wrap items-baseline justify-between gap-2 text-sm">
					<span className="text-text-secondary">
						{chosen.length} of {sampleCount} sample{sampleCount === 1 ? "" : "s"}
					</span>
					{availableMemoryBytes ? (
						<span className="text-xs text-text-muted font-mono tabular-nums">
							{formatBytes(availableMemoryBytes)} available
						</span>
					) : null}
				</div>

				{previous ? (
					<p className="text-xs text-text-muted">
						Last run {formatRelativeTime(previous.started_at)} on {previous.app_version} /
						transcribe-cpp {previous.engine_version},{" "}
						{previous.language ? `hint ${previous.language}` : "no hint"}. Output changes are only
						comparable against a run that already exists.
					</p>
				) : (
					<p className="text-xs text-warning">
						This is the first run, so nothing can be compared against it yet. Run one before your
						next upgrade to get a baseline.
					</p>
				)}

				{/* Two lists, side by side: stacked, each one pushed the other
				    and the Start button off the bottom of the dialog. */}
				<div className="grid gap-5 md:grid-cols-2">
					<section className="space-y-2 min-w-0">
						<PickerHead
							title="Models"
							chosen={shownModels.filter((m: ModelInfo) => selected.has(m.id)).length}
							shown={shownModels.length}
							total={installed.length}
							onAll={selectAllModels}
							onNone={selectNoModels}
						/>
						<SearchBox
							value={modelQuery}
							onChange={setModelQuery}
							placeholder="Search name or language…"
						/>
						<div className="divide-y divide-border max-h-72 overflow-y-auto">
							{installed.length === 0 ? (
								<p className="py-6 text-center text-sm text-text-muted">
									No installed models. Download candidates on the Models page first.
								</p>
							) : shownModels.length === 0 ? (
								<p className="py-6 text-center text-sm text-text-muted">
									No installed model matches “{modelQuery}”.
								</p>
							) : (
								shownModels.map((m: ModelInfo) => (
									<div
										key={m.id}
										className="w-full flex items-center gap-3 py-2 px-1 hover:bg-surface-3"
									>
										<button
											type="button"
											onClick={() => toggle(m.id)}
											className="flex-1 min-w-0 flex items-center gap-3 text-left cursor-pointer"
										>
											<span className="w-4 shrink-0 text-center font-mono text-xs">
												{selected.has(m.id) ? "\u2611" : "\u2610"}
											</span>
											<span className="flex-1 min-w-0 truncate text-sm text-text-primary">
												{m.id}
											</span>
											<span className="text-xs text-text-muted font-mono tabular-nums">
												{m.size_mb}MB
											</span>
										</button>
										<Hint
											label="Languages"
											content={m.languages.join(", ")}
											hitPad={false}
											className="w-24 shrink-0 justify-end overflow-hidden text-xs text-text-muted"
										>
											<span className="truncate">
												{m.languages.slice(0, 2).join(", ")}
												{m.languages.length > 2 ? ` +${m.languages.length - 2}` : ""}
											</span>
										</Hint>
										{m.is_loaded && <span className="text-xs text-accent shrink-0">loaded</span>}
									</div>
								))
							)}
						</div>
					</section>

					<section className="space-y-2 min-w-0">
						<PickerHead
							title="Samples"
							chosen={shownSamples.filter((s) => !excluded.has(s.id)).length}
							shown={shownSamples.length}
							total={samples.length}
							onAll={includeAllSamples}
							onNone={excludeAllSamples}
						/>
						<SearchBox
							value={sampleQuery}
							onChange={setSampleQuery}
							placeholder="Search transcript or device…"
						/>
						<div className="divide-y divide-border max-h-72 overflow-y-auto">
							{shownSamples.length === 0 ? (
								<p className="py-6 text-center text-sm text-text-muted">
									{samples.length === 0
										? "No samples yet. Add recordings from History or the Samples tab."
										: `No sample matches “${sampleQuery}”.`}
								</p>
							) : (
								shownSamples.map((s) => (
									<button
										type="button"
										key={s.id}
										onClick={() =>
											setExcluded((prev) => {
												const next = new Set(prev);
												if (next.has(s.id)) next.delete(s.id);
												else next.add(s.id);
												return next;
											})
										}
										className="w-full flex items-center gap-3 py-1.5 px-1 text-left cursor-pointer hover:bg-surface-3"
									>
										<span className="w-4 shrink-0 text-center font-mono text-xs">
											{excluded.has(s.id) ? "\u2610" : "\u2611"}
										</span>
										<span
											className={`flex-1 min-w-0 truncate text-sm ${
												excluded.has(s.id) ? "text-text-muted" : "text-text-primary"
											}`}
										>
											{s.reference_transcript}
										</span>
										<span className="text-xs text-text-muted truncate max-w-[7rem]">
											{s.capture_device ?? "unknown"}
										</span>
										<span className="text-xs text-text-muted font-mono tabular-nums w-12 text-right shrink-0">
											{formatDuration(s.audio_duration_ms)}
										</span>
									</button>
								))
							)}
						</div>
					</section>
				</div>

				<p className="text-xs text-text-muted">
					The set holds different scenarios and different languages, and a run carries one language
					hint — so pick the samples that belong together rather than scoring English clips under a
					Chinese hint.
				</p>

				<Input
					label="Language hint (optional)"
					value={language}
					placeholder="auto — e.g. zh-TW, en, ja"
					onChange={(e) => setLanguage(e.target.value)}
				/>
				<p className="text-xs text-text-muted">
					One hint for the whole run. Giving models different hints would compare setups rather than
					models, and a run is only compared against an earlier one that used the same hint — which
					is why this starts on whatever the last run used.
				</p>
			</div>
		</Modal>
	);
}

/**
 * A list's title, what it currently contributes to the run, and the two
 * bulk actions.
 *
 * `chosen` counts only rows in view, so with a search active the number
 * and the buttons describe the same set — the totals for the run itself
 * are on the line above the two lists.
 */
function PickerHead({
	title,
	chosen,
	shown,
	total,
	onAll,
	onNone,
}: {
	title: string;
	chosen: number;
	shown: number;
	total: number;
	onAll: () => void;
	onNone: () => void;
}) {
	return (
		<div className="flex items-baseline justify-between gap-2">
			<span className="text-sm font-medium text-text-secondary">{title}</span>
			<div className="flex items-baseline gap-2.5 text-xs">
				<span className="num text-text-muted tabular-nums">
					{shown === total ? `${chosen}/${total}` : `${chosen}/${shown} of ${total}`}
				</span>
				<button type="button" onClick={onAll} className="text-accent cursor-pointer">
					All
				</button>
				<button
					type="button"
					onClick={onNone}
					className="text-text-muted hover:text-text-secondary cursor-pointer"
				>
					None
				</button>
			</div>
		</div>
	);
}
