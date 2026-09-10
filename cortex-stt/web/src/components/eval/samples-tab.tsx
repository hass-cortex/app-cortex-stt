import { Mic, Play, RotateCcw, Trash2, Upload } from "lucide-react";
import { useRef, useState } from "react";
import { evalAudioUrl } from "@/api/client";
import type { EvalSampleListEntry, SampleSetComposition } from "@/api/types";
import { ClipPlayer } from "@/components/audio/clip-player";
import { ImportPicker } from "@/components/eval/import-picker";
import { Labeller } from "@/components/eval/labeller";
import { ReferenceEditor } from "@/components/eval/reference-editor";
import { SampleHistory } from "@/components/eval/sample-history";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { useConfirm } from "@/components/ui/confirm";
import { EmptyState } from "@/components/ui/empty-state";
import { Hint } from "@/components/ui/hint";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { SortButton } from "@/components/ui/sort-header";
import { useToast } from "@/components/ui/toast";
import { useDeleteSample, useDeleteSamples, useSamples, useUploadCapture } from "@/hooks/use-eval";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useSelection } from "@/hooks/use-selection";
import { type SortColumn, useTableSort } from "@/hooks/use-table-sort";
import { clipFileName, formatDuration } from "@/lib/format";

/** What each column of the set contributes to the ordering. */
const SORT_COLUMNS: Record<string, SortColumn<EvalSampleListEntry>> = {
	reference: { value: (s) => s.reference_transcript, first: "asc" },
	// Zero is a real answer here — "never measured" is what you sort to
	// find — so it stays a number rather than becoming a null that sinks.
	runs: { value: (s) => s.run_count, first: "desc" },
	device: { value: (s) => s.capture_device ?? "", first: "asc" },
	duration: { value: (s) => s.audio_duration_ms, first: "desc" },
};

export function SamplesTab({
	composition,
	onRunSamples,
}: {
	composition?: SampleSetComposition;
	/** Hand a selection to the New run dialog, which the page owns. */
	onRunSamples?: (sampleIds: string[]) => void;
}) {
	const { data: samples = [] } = useSamples();
	const [pickerOpen, setPickerOpen] = useState(false);
	const [expanded, setExpanded] = useState<string | null>(null);
	const fileInput = useRef<HTMLInputElement>(null);
	const upload = useUploadCapture();
	const remove = useDeleteSample();

	const runUpload = useMutationToast(upload, {
		success: "Added to labelling",
		error: "Could not add the file",
	});
	const runRemove = useMutationToast(remove, {
		success: "Sample deleted",
		error: "Could not delete",
	});

	// Deleting cascades into every run that used the sample, so the count
	// goes in front of the click rather than being discovered afterwards.
	const confirm = useConfirm();
	const { toast } = useToast();

	const confirmRemove = async (s: EvalSampleListEntry) => {
		const ok = await confirm({
			title: "Delete this sample?",
			body: (
				<>
					<p className="text-text-primary">“{s.reference_transcript}”</p>
					{s.result_count > 0 && (
						<p className="mt-2">
							This also removes {s.result_count} result(s) from {s.run_count} run(s). Those runs
							will then report fewer samples than they measured.
						</p>
					)}
				</>
			),
			confirmLabel: "Delete sample",
			destructive: true,
		});
		if (ok) runRemove(s.id);
	};
	const [text, setText] = useState("");
	const [device, setDevice] = useState("");

	// Facets from the set itself, so a filter never offers a device that
	// recorded nothing.
	const devices = [...new Set(samples.map((s) => s.capture_device ?? ""))].sort();
	const needle = text.trim().toLowerCase();
	const shown = samples.filter((s) => {
		if (device && (s.capture_device ?? "") !== device) return false;
		if (!needle) return true;
		return s.reference_transcript.toLowerCase().includes(needle);
	});
	const filtered = !!needle || !!device;
	// Sorting sits on top of the filters: the search decides which rows
	// exist, the header decides what order they arrive in.
	const sort = useTableSort(shown, SORT_COLUMNS);

	const selection = useSelection();
	const removeMany = useDeleteSamples();
	const shownIds = shown.map((s) => s.id);
	const allShownSelected = shownIds.length > 0 && shownIds.every(selection.has);
	// Everything ticked, including rows the filter is currently hiding —
	// the number the confirmation has to be about.
	const selectedSamples = samples.filter((s) => selection.has(s.id));
	const hiddenSelected = selectedSamples.length - shownIds.filter(selection.has).length;

	const selectAllBox = (
		<Checkbox
			checked={allShownSelected}
			indeterminate={!allShownSelected && shownIds.some(selection.has)}
			onChange={() => (allShownSelected ? selection.remove(shownIds) : selection.add(shownIds))}
			label={allShownSelected ? "Deselect the rows shown" : "Select the rows shown"}
		/>
	);

	const confirmRemoveMany = async () => {
		const results = selectedSamples.reduce((n, s) => n + s.result_count, 0);
		const measured = selectedSamples.filter((s) => s.run_count > 0).length;
		const ok = await confirm({
			title: `Delete ${selectedSamples.length} sample(s)?`,
			body: (
				<>
					{results > 0 && (
						<p>
							This also removes {results} result(s). {measured} of these sample(s) have been
							measured, and the runs that used them will then report fewer samples than they
							measured.
						</p>
					)}
					{hiddenSelected > 0 && (
						<p className="mt-2 text-warning">
							{hiddenSelected} of them are not shown by the current filter.
						</p>
					)}
				</>
			),
			confirmLabel: `Delete ${selectedSamples.length}`,
			destructive: true,
		});
		if (!ok) return;
		removeMany.mutate(
			selectedSamples.map((s) => s.id),
			{
				onSuccess: (data) => {
					toast(`${data.deleted} sample(s) deleted`, "success");
					selection.clear();
				},
				onError: (err) => toast(`Could not delete: ${err.message}`, "error"),
			},
		);
	};

	return (
		<div className="space-y-6">
			<Labeller />

			<Card>
				<CardHeader
					title="Evaluation set"
					description={`${samples.length} sample(s), ${formatDuration(composition?.total_duration_ms ?? 0)} of audio`}
					action={
						<div className="flex gap-2">
							<Button size="sm" variant="secondary" onClick={() => setPickerOpen(true)}>
								Add from history
							</Button>
							<Button
								size="sm"
								variant="secondary"
								icon={<Upload size={14} />}
								loading={upload.isPending}
								onClick={() => fileInput.current?.click()}
							>
								Upload
							</Button>
						</div>
					}
				/>

				<input
					ref={fileInput}
					type="file"
					accept="audio/wav,.wav"
					className="hidden"
					onChange={(e) => {
						const file = e.target.files?.[0];
						if (file) runUpload({ file });
						e.target.value = "";
					}}
				/>

				<CoverageBar composition={composition} />

				{samples.length > 0 && (
					<div className="flex flex-col sm:flex-row gap-3 pt-3">
						<Input
							type="text"
							placeholder="Search reference text..."
							value={text}
							onChange={(e) => setText(e.target.value)}
							className="sm:w-64"
						/>
						<Select
							options={[
								{ value: "", label: "All capture devices" },
								...devices.map((d) => ({ value: d, label: d || "unknown" })),
							]}
							value={device}
							onChange={(e) => setDevice(e.target.value)}
							className="sm:w-56"
						/>
						{filtered && (
							<span className="self-center text-xs text-text-muted whitespace-nowrap">
								{shown.length} of {samples.length}
							</span>
						)}
						{/* The set arrives oldest-first, which is the order it was
						    built in — worth one click back to. */}
						{sort.active && (
							<button
								type="button"
								onClick={sort.reset}
								aria-label="Sort by the order samples were added"
								className="self-center sm:ml-auto inline-flex items-center gap-1 text-[11px] text-text-muted hover:text-text-primary cursor-pointer transition-colors"
							>
								<RotateCcw size={11} strokeWidth={1.8} />
								Added order
							</button>
						)}
					</div>
				)}

				{samples.length === 0 ? (
					<EmptyState
						icon={<Mic size={28} />}
						title="No samples yet"
						description="Pick recordings from history, or upload a clip you recorded on purpose. Only lossless audio is accepted."
					/>
				) : (
					// Same gap the History list keeps between its filters and its
					// table; without it the header band touches the search box.
					<div className="mt-4 divide-y divide-border">
						{/* Only where the row is columnar. Below `sm` the metrics wrap
						    under the transcript, so there are no columns to head.

						    A live selection covers this row rather than pushing it
						    down: a bar entering the flow shifts every row the moment
						    the first tick lands. */}
						<div className="hidden sm:flex sm:items-center sm:gap-3 pb-1.5 text-xs text-text-muted">
							{selectAllBox}
							{/* The cover starts after the tick box, so that stays the one
							    control it is instead of gaining a copy underneath. */}
							{/* Tall enough for the Delete button the cover puts here, in
							    both states: a band shorter than its own content would let
							    that button hang out over the row above. */}
							<div className="relative flex-1 min-w-0 flex min-h-[26px] items-center gap-3">
								{selection.size > 0 && (
									<div className="absolute inset-0 z-10 flex items-center gap-3 bg-surface-2">
										<span className="text-text-primary tabular-nums">
											{selection.size} selected
											{hiddenSelected > 0 && (
												<span className="text-warning"> ({hiddenSelected} not shown)</span>
											)}
										</span>
										{onRunSamples && (
											<Button
												size="sm"
												variant="secondary"
												icon={<Play size={13} strokeWidth={1.8} />}
												onClick={() => onRunSamples(selectedSamples.map((x) => x.id))}
											>
												New run
											</Button>
										)}
										<Button
											size="sm"
											variant="danger"
											icon={<Trash2 size={13} strokeWidth={1.8} />}
											loading={removeMany.isPending}
											onClick={confirmRemoveMany}
										>
											Delete
										</Button>
										<button
											type="button"
											onClick={selection.clear}
											className="text-text-muted hover:text-text-primary cursor-pointer"
										>
											Clear
										</button>
									</div>
								)}
								<SortButton
									column="reference"
									active={sort.active}
									onToggle={sort.toggle}
									className="flex-1 min-w-0 justify-start"
								>
									Reference
								</SortButton>
								<SortButton
									column="runs"
									active={sort.active}
									onToggle={sort.toggle}
									className="w-16 shrink-0 justify-end"
								>
									Runs
								</SortButton>
								<SortButton
									column="device"
									active={sort.active}
									onToggle={sort.toggle}
									className="w-36 shrink-0 justify-end"
								>
									Device
								</SortButton>
								<SortButton
									column="duration"
									active={sort.active}
									onToggle={sort.toggle}
									className="w-20 shrink-0 justify-end"
								>
									Length
								</SortButton>
								{/* Reserves the delete button's width so the headings above
								    sit over their own values. */}
								<span className="w-8 shrink-0" />
							</div>
						</div>
						{sort.rows.map((s) => (
							<div key={s.id} className="py-2">
								{/* The reference transcript is the row's subject; on a narrow
								    screen it takes the line and the metrics move under it
								    rather than squeezing it to one character per line. */}
								<div className="flex flex-col gap-1 sm:flex-row sm:items-center sm:gap-3">
									<Checkbox
										checked={selection.has(s.id)}
										onChange={() => selection.toggle(s.id)}
										label={`Select “${s.reference_transcript}”`}
										className="hidden sm:inline-flex"
									/>
									<button
										type="button"
										onClick={() => setExpanded(expanded === s.id ? null : s.id)}
										className="flex-1 min-w-0 text-left cursor-pointer"
									>
										<span className="text-sm text-text-primary">{s.reference_transcript}</span>
									</button>
									<div className="flex items-center gap-3 shrink-0">
										{/* Always rendered, unlike before: a column that appears
										    only on some rows cannot carry a heading, and "never
										    run" is the value most worth sorting to. */}
										<Hint
											label="Runs"
											content={
												s.run_count > 0
													? `${s.result_count} result(s) across ${s.run_count} run(s)`
													: "Never measured by a run"
											}
											width={240}
											hitPad={false}
											className="w-16 shrink-0 justify-end text-xs text-text-muted whitespace-nowrap font-mono tabular-nums"
										>
											{/* The heading says what the number counts, so the unit
											    per row was 27 repetitions of the word "run". Zero is
											    a count like any other here, not a missing value. */}
											{s.run_count}
										</Hint>
										<span className="w-36 shrink-0 text-right text-xs text-text-muted whitespace-nowrap truncate">
											{s.capture_device ?? "unknown"}
										</span>
										<span className="text-xs text-text-muted font-mono tabular-nums w-20 text-right shrink-0">
											{formatDuration(s.audio_duration_ms)}
										</span>
										<Button
											size="sm"
											variant="ghost"
											icon={<Trash2 size={14} />}
											onClick={() => confirmRemove(s)}
											className="ml-auto sm:ml-0 w-8 shrink-0"
										/>
									</div>
								</div>
								{expanded === s.id && (
									<div className="pt-2 pb-1 space-y-3">
										<ClipPlayer
											src={evalAudioUrl(s.id)}
											durationMs={s.audio_duration_ms}
											downloadName={clipFileName(
												"sample",
												s.reference_transcript,
												s.id.slice(0, 8),
											)}
										/>
										<ReferenceEditor sample={s} />
										<SampleHistory sample={s} />
									</div>
								)}
							</div>
						))}
					</div>
				)}
			</Card>

			<ImportPicker open={pickerOpen} onClose={() => setPickerOpen(false)} />
		</div>
	);
}

/**
 * What the set is actually made of.
 *
 * The set is curated by hand from whatever happened to be in history,
 * so its coverage skews without anyone choosing that. A per-microphone
 * comparison drawn from a set that is 90% one microphone answers a
 * different question than the one asked.
 */
function CoverageBar({ composition }: { composition?: SampleSetComposition }) {
	if (!composition || composition.total === 0) return null;
	const devices = composition.by_capture_device;

	return (
		<div className="mb-4 space-y-2">
			<div className="flex h-2 rounded-full overflow-hidden bg-surface-3">
				{devices.map((d, i) => (
					<div
						key={d.capture_device}
						className={i % 2 === 0 ? "bg-accent" : "bg-info"}
						style={{ width: `${(d.count / composition.total) * 100}%` }}
					/>
				))}
			</div>
			<div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-text-muted">
				{devices.map((d) => (
					<span key={d.capture_device}>
						{d.capture_device} <span className="font-mono tabular-nums">{d.count}</span>
					</span>
				))}
				{devices.length === 1 && (
					<span className="text-warning">
						Every sample came from one microphone — per-microphone comparisons are not possible yet.
					</span>
				)}
			</div>
		</div>
	);
}
