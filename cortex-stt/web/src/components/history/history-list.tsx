import {
	AlertTriangle,
	ChevronDown,
	ChevronRight,
	FlaskConical,
	History,
	Trash2,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";
import { ApiClientError, audioUrl } from "@/api/client";
import type { HistoryFilters, TranscriptionRecord } from "@/api/types";
import { ClipPlayer } from "@/components/audio/clip-player";
import { ClipTimeline, segmentsDivideClip } from "@/components/audio/clip-timeline";
import { ReferenceField } from "@/components/eval/reference-field";
import { BatchLabelDialog } from "@/components/history/batch-label-dialog";
import { RawOutput } from "@/components/transcript/raw-output";
import { RecordCells, RecordHead, rtfOf } from "@/components/transcript/record-row";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useConfirm } from "@/components/ui/confirm";
import { EmptyState } from "@/components/ui/empty-state";
import { SearchBox } from "@/components/ui/search-box";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { useToast } from "@/components/ui/toast";
import { useCaptureRecord, useLabelRecord, useTakenOrigins } from "@/hooks/use-eval";
import {
	useDeleteHistoryRecord,
	useDeleteHistoryRecords,
	useHistoryFacets,
	useHistoryList,
} from "@/hooks/use-history";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useSelection } from "@/hooks/use-selection";
import { useSettings } from "@/hooks/use-settings";
import { formatClipRatio, formatDbfs, isQuiet, isQuietAndSilent } from "@/lib/capture-level";
import { clipFileName, formatDuration } from "@/lib/format";
import { formatTimestamp, timestampSlug } from "@/utils/time";

const errorOptions = [
	{ value: "", label: "All results" },
	{ value: "true", label: "Errors only" },
	{ value: "false", label: "Successful only" },
];

const rangeOptions = [
	{ value: "", label: "All time" },
	{ value: "24", label: "Last 24 hours" },
	{ value: "168", label: "Last 7 days" },
	{ value: "720", label: "Last 30 days" },
];

/** How many more rows each request for older records asks for. */
const PAGE = 50;

export function HistoryList() {
	const [range, setRange] = useState("");
	// Raising the limit rather than paging with an offset: the endpoint
	// returns a flat array, and re-reading a longer window keeps one source
	// of truth instead of a client-side accumulation that drifts as records
	// arrive.
	const [limit, setLimit] = useState(PAGE);
	const [filters, setFilters] = useState<HistoryFilters>({});
	// `?record=` is how the dashboard's recent list links here: the same
	// row, opened. Kept in sync on every toggle so the URL always names
	// what is on screen.
	const [params, setParams] = useSearchParams();
	const linkedId = params.get("record");
	const [expandedId, setExpandedId] = useState<string | null>(linkedId);
	const { data, isLoading, error } = useHistoryList({ ...filters, limit });
	const { data: facets } = useHistoryFacets();
	const { data: settings } = useSettings();
	const timezone = settings?.timezone ?? "auto";

	// A linked record can sit well below the fold. Once only — a later
	// click is the reader's own scroll position to keep.
	const broughtIntoView = useRef(false);
	useEffect(() => {
		if (broughtIntoView.current || !linkedId || isLoading) return;
		broughtIntoView.current = true;
		document.getElementById(`record-${linkedId}`)?.scrollIntoView({ block: "center" });
	}, [linkedId, isLoading]);

	const toggleExpanded = (id: string) => {
		const next = expandedId === id ? null : id;
		setExpandedId(next);
		if (next) {
			params.set("record", next);
		} else {
			params.delete("record");
		}
		setParams(params, { replace: true });
	};

	const confirm = useConfirm();
	const { toast } = useToast();
	const selection = useSelection();
	const removeMany = useDeleteHistoryRecords();
	const { data: takenOrigins = [] } = useTakenOrigins();
	const [labelBatchOpen, setLabelBatchOpen] = useState(false);

	const updateFilter = (key: keyof HistoryFilters, value: string) => {
		setLimit(PAGE);
		setFilters((prev) => ({ ...prev, [key]: value === "" ? undefined : value }));
	};

	const updateRange = (hours: string) => {
		setRange(hours);
		setLimit(PAGE);
		setFilters((prev) => ({
			...prev,
			from: hours ? new Date(Date.now() - Number(hours) * 3_600_000).toISOString() : undefined,
		}));
	};

	if (error) {
		return (
			<EmptyState
				icon={<History size={40} />}
				title="Failed to load history"
				description={error.message}
			/>
		);
	}

	const records = data ?? [];
	// Which recordings the evaluation set already holds. Without it the
	// only way to find out is to send a batch and read the refusals back.
	const taken = new Set(takenOrigins);

	const shownIds = records.map((r) => r.id);
	const allShownSelected = shownIds.length > 0 && shownIds.every(selection.has);
	// A tick survives a filter change, so the count can exceed what is on
	// screen. The confirmation says so rather than letting the number be
	// the only clue.
	const hiddenSelected = selection.size - shownIds.filter(selection.has).length;

	const selectAllBox = (
		<Checkbox
			checked={allShownSelected}
			indeterminate={!allShownSelected && shownIds.some(selection.has)}
			onChange={() => (allShownSelected ? selection.remove(shownIds) : selection.add(shownIds))}
			label={allShownSelected ? "Deselect the rows shown" : "Select the rows shown"}
			className="hidden sm:inline-flex"
		/>
	);

	const confirmRemoveMany = async () => {
		const ids = [...selection.ids];
		const ok = await confirm({
			title: `Delete ${ids.length} record(s)?`,
			body: (
				<>
					<p>
						The transcripts and any audio still kept for them both go. Evaluation samples taken from
						them are unaffected — a sample owns its own copy.
					</p>
					{hiddenSelected > 0 && (
						<p className="mt-2 text-warning">
							{hiddenSelected} of them are not shown by the current filter.
						</p>
					)}
				</>
			),
			confirmLabel: `Delete ${ids.length}`,
			destructive: true,
		});
		if (!ok) return;
		removeMany.mutate(ids, {
			onSuccess: (data) => {
				toast(`${data.deleted} record(s) deleted`, "success");
				selection.clear();
			},
			onError: (err) => toast(`Could not delete: ${err.message}`, "error"),
		});
	};

	return (
		<div className="flex flex-col gap-4">
			<div className="flex flex-col sm:flex-row gap-2.5">
				<SearchBox
					value={filters.text ?? ""}
					onChange={(value) => updateFilter("text", value)}
					placeholder="Search transcripts…"
					className="flex-1"
				/>
				<Select
					options={errorOptions}
					value={filters.has_error !== undefined ? String(filters.has_error) : ""}
					onChange={(e) => updateFilter("has_error", e.target.value)}
					className="sm:w-32"
				/>
				<Select
					options={[
						{ value: "", label: "All capture devices" },
						...(facets?.capture_devices ?? []).map((d) => ({ value: d, label: d })),
					]}
					value={filters.capture_device ?? ""}
					onChange={(e) => updateFilter("capture_device", e.target.value)}
					className="sm:w-44"
				/>
				<Select
					options={[
						{ value: "", label: "All models" },
						...(facets?.models ?? []).map((m) => ({ value: m, label: m })),
					]}
					value={filters.model ?? ""}
					onChange={(e) => updateFilter("model", e.target.value)}
					className="sm:w-44"
				/>
				<Select
					options={rangeOptions}
					value={range}
					onChange={(e) => updateRange(e.target.value)}
					className="sm:w-36"
				/>
			</div>

			<BatchLabelDialog
				open={labelBatchOpen}
				onClose={() => setLabelBatchOpen(false)}
				records={records.filter((r) => selection.has(r.id))}
				hiddenIds={[...selection.ids].filter((id) => !shownIds.includes(id))}
				taken={taken}
				timezone={timezone}
				onDone={selection.clear}
			/>

			{isLoading ? (
				<div className="flex justify-center py-16">
					<Spinner size="lg" />
				</div>
			) : records.length === 0 ? (
				<EmptyState
					icon={<History size={40} />}
					title="No transcriptions here"
					description="Records appear as audio is processed. Adjust the filters if you expected some."
				/>
			) : (
				<div className="bg-surface-2 border border-border rounded-[10px] overflow-hidden">
					<RecordHead
						select={selectAllBox}
						actions={
							selection.size > 0 ? (
								<>
									<span className="num text-[11px] text-text-primary tabular-nums">
										{selection.size} selected
										{hiddenSelected > 0 && (
											<span className="text-warning"> ({hiddenSelected} not shown)</span>
										)}
									</span>
									<Button
										size="sm"
										variant="secondary"
										icon={<FlaskConical size={13} strokeWidth={1.8} />}
										onClick={() => setLabelBatchOpen(true)}
									>
										Add to evaluation
									</Button>
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
										className="text-[11px] text-text-muted hover:text-text-primary cursor-pointer"
									>
										Clear
									</button>
								</>
							) : undefined
						}
					/>

					{records.map((record) => (
						<HistoryRow
							key={record.id}
							record={record}
							isExpanded={expandedId === record.id}
							onToggle={() => toggleExpanded(record.id)}
							selected={selection.has(record.id)}
							onSelect={() => selection.toggle(record.id)}
							inEvalSet={taken.has(record.id)}
							timezone={timezone}
						/>
					))}

					<div className="flex items-center justify-between gap-3 px-3.5 py-2.5 bg-surface-1 border-t border-border">
						<span className="num text-[11px] text-text-faint">
							showing {records.length}
							{records.length < limit ? " — the end of this filter" : ""}
						</span>
						{records.length === limit && (
							<Button variant="outline" size="sm" onClick={() => setLimit((n) => n + PAGE)}>
								Load {PAGE} more
							</Button>
						)}
					</div>
				</div>
			)}
		</div>
	);
}

function HistoryRow({
	record,
	isExpanded,
	onToggle,
	selected,
	onSelect,
	inEvalSet,
	timezone,
}: {
	record: TranscriptionRecord;
	isExpanded: boolean;
	onToggle: () => void;
	selected: boolean;
	onSelect: () => void;
	inEvalSet: boolean;
	timezone: string;
}) {
	return (
		<div
			id={`record-${record.id}`}
			className={`border-b border-border-soft last:border-0 ${isExpanded ? "bg-surface-1" : ""}`}
		>
			{/* The tick box is a sibling of the row button, not inside it: a
			    control nested in a button is neither clickable nor valid. */}
			<div
				className={`flex items-center gap-3.5 px-3 hover:bg-surface-3/40 transition-colors ${
					isExpanded ? "shadow-[inset_2px_0_0_var(--accent)]" : ""
				}`}
			>
				<Checkbox
					checked={selected}
					onChange={onSelect}
					label={`Select the transcription from ${record.timestamp}`}
					className="hidden sm:inline-flex"
				/>
				<button
					type="button"
					onClick={onToggle}
					className="flex-1 min-w-0 flex items-center gap-3.5 py-2.5 text-left cursor-pointer"
				>
					{isExpanded ? (
						<ChevronDown size={14} strokeWidth={1.8} className="text-accent-ink shrink-0" />
					) : (
						<ChevronRight size={14} strokeWidth={1.8} className="text-text-faint shrink-0" />
					)}
					<RecordCells
						record={record}
						timezone={timezone}
						mark={
							// Named after the button that put it there, not after the
							// table it landed in: "in evaluation" is the same phrase as
							// "Add to evaluation", and the flask is the same icon.
							inEvalSet ? (
								<span className="shrink-0 inline-flex items-center gap-1 text-[10.5px] text-accent-ink whitespace-nowrap">
									<FlaskConical size={11} strokeWidth={1.8} />
									in evaluation
								</span>
							) : undefined
						}
					/>
				</button>
			</div>

			{isExpanded && <ExpandedDetail record={record} timezone={timezone} />}
		</div>
	);
}

function Meta({ label, value, tone }: { label: string; value: string; tone?: "warn" }) {
	return (
		<div className="flex flex-col gap-[3px]">
			<span className="num text-[10px] tracking-[0.06em] text-text-faint">{label}</span>
			<span
				className={`num text-[12px] break-words ${tone === "warn" ? "text-warning" : "text-text-primary"}`}
			>
				{value}
			</span>
		</div>
	);
}

function ExpandedDetail({ record, timezone }: { record: TranscriptionRecord; timezone: string }) {
	const confirm = useConfirm();
	const deleteMutation = useDeleteHistoryRecord();
	const runDelete = useMutationToast(deleteMutation, { success: "Record deleted" });
	const captureMutation = useCaptureRecord();
	const labelMutation = useLabelRecord();
	const { toast } = useToast();

	// The server names the record in its message, which is right for an
	// API and wrong here: the row is on screen, so the id is noise and
	// "already in the set" is the whole answer.
	const reportFailure = (err: Error) =>
		toast(
			err instanceof ApiClientError && err.code === "EVAL_SAMPLE_EXISTS"
				? "Already in the evaluation set"
				: `Not added: ${err.message}`,
			"error",
		);
	const runCapture = (id: string) =>
		captureMutation.mutate(id, {
			onSuccess: () => toast("Added to labelling", "success"),
			onError: reportFailure,
		});
	const runLabel = (vars: { recordId: string; reference: string }) =>
		labelMutation.mutate(vars, {
			onSuccess: () => toast("Sample added", "success"),
			onError: reportFailure,
		});
	// The reference is typed here rather than on the Evaluation page: this
	// is the screen where the clip can be played and the model's own
	// transcript is on the line above, which is what makes the reference
	// quick to type and quick to check.
	const [labelling, setLabelling] = useState(false);
	const [reference, setReference] = useState("");
	const closeLabelling = () => {
		setLabelling(false);
		setReference("");
	};

	const hasAudio = !!record.audio_path;
	const quiet = isQuiet(record.rms_db);
	const clipped = record.clip_ratio ?? 0;

	return (
		<div className="flex flex-col xl:flex-row gap-4 px-3 pb-4 pl-10">
			<div className="flex-1 min-w-0 flex flex-col gap-3">
				{hasAudio ? (
					<ClipPlayer
						src={audioUrl(record.id)}
						durationMs={record.audio_duration_ms}
						downloadName={clipFileName(timestampSlug(record.timestamp, timezone), record.model_id)}
						segments={record.segments}
						meta={`${record.segments.length} ${record.segments.length === 1 ? "segment" : "segments"} · ${record.device?.toUpperCase() ?? "CPU"}`}
					/>
				) : (
					// Without audio the block is worth showing only for the timings
					// themselves — and only when they divide the clip into parts the
					// transcript below does not already state.
					segmentsDivideClip(record.segments, record.audio_duration_ms) && (
						<div className="p-3.5 bg-surface-3 rounded-lg">
							<p className="num text-[11px] text-text-muted mb-3">
								Audio was dropped by retention — the segments keep their timings, but there is
								nothing left to play.
							</p>
							{/* No transport: a play button here could never do anything. */}
							<ClipTimeline
								peaks={[]}
								durationMs={record.audio_duration_ms}
								segments={record.segments}
								playhead={false}
								height={24}
							/>
						</div>
					)
				)}

				{/* A failed request has no transcript to show — saying "empty"
				    above the error that explains why just says it twice. */}
				{record.has_error ? (
					<div className="px-3 py-2.5 bg-error-wash border border-error/30 rounded-lg">
						<div className="num text-[10px] tracking-[0.06em] text-error/70">FAILED</div>
						<p className="mt-1.5 text-[13px] leading-relaxed text-error">
							{record.error_message ?? "The request failed without a message."}
						</p>
					</div>
				) : (
					<div className="px-3 py-2.5 bg-surface-3 rounded-lg select-text">
						<div className="num text-[10px] tracking-[0.06em] text-text-faint">TRANSCRIPT</div>
						<p className="mt-1.5 text-[14px] leading-relaxed text-text-primary whitespace-pre-wrap">
							{record.text || <span className="text-text-muted italic">Empty transcript</span>}
						</p>
					</div>
				)}

				{record.raw_text && <RawOutput text={record.raw_text} />}

				<div className="flex gap-2 flex-wrap">
					{hasAudio && (
						<Button
							variant="secondary"
							size="sm"
							icon={<FlaskConical size={13} strokeWidth={1.8} />}
							onClick={() => (labelling ? closeLabelling() : setLabelling(true))}
						>
							Add to evaluation
						</Button>
					)}
					<Button
						variant="danger"
						size="sm"
						icon={<Trash2 size={13} strokeWidth={1.8} />}
						className="ml-auto"
						loading={deleteMutation.isPending}
						onClick={async () => {
							const ok = await confirm({
								title: "Delete this record?",
								body: hasAudio
									? "The transcript and its audio file both go. Evaluation samples taken from it are unaffected — a sample owns its own copy."
									: "The transcript goes. This record has no audio left to remove.",
								confirmLabel: "Delete record",
								destructive: true,
							});
							if (ok) runDelete(record.id);
						}}
					>
						Delete record
					</Button>
				</div>

				{labelling && hasAudio && (
					<div className="p-3.5 bg-surface-3 rounded-lg space-y-3">
						<ReferenceField
							id={`reference-${record.id}`}
							value={reference}
							onChange={setReference}
							suggestion={record.text || null}
						/>
						<div className="flex gap-2 flex-wrap">
							<Button
								size="sm"
								disabled={!reference.trim()}
								loading={labelMutation.isPending}
								onClick={() => {
									runLabel({ recordId: record.id, reference });
									closeLabelling();
								}}
							>
								Add sample
							</Button>
							{/* The capture-only path the Evaluation page still advertises:
							    take the clip now, say what it was when there is time. */}
							<Button
								size="sm"
								variant="ghost"
								loading={captureMutation.isPending}
								onClick={() => {
									runCapture(record.id);
									closeLabelling();
								}}
							>
								Label later
							</Button>
							<Button size="sm" variant="ghost" className="ml-auto" onClick={closeLabelling}>
								Cancel
							</Button>
						</div>
					</div>
				)}
			</div>

			<div className="w-full xl:w-[384px] shrink-0 flex flex-col gap-3">
				<div className="px-3.5 py-3 bg-surface-2 border border-border rounded-lg">
					<div className="grid grid-cols-3 gap-x-3.5 gap-y-3">
						{/* The model column is hidden on narrow screens, so without
						    this the expanded row cannot say what produced the text. */}
						<Meta label="MODEL" value={record.model_id} />
						<Meta label="LANGUAGE" value={record.language ?? "auto"} />
						<Meta label="TIMESTAMP" value={formatTimestamp(record.timestamp, timezone)} />
						<Meta label="AUDIO" value={formatDuration(record.audio_duration_ms)} />
						<Meta label="RTF" value={rtfOf(record)} />
						<Meta label="INFERENCE" value={formatDuration(record.inference_ms)} />
						<Meta label="POOL WAIT" value={formatDuration(record.pool_wait_ms)} />
						<Meta
							label="ACQUIRE"
							value={formatDuration(record.cold_load_ms)}
							tone={record.cold_load_ms > 0 ? "warn" : undefined}
						/>
						<Meta label="BACKEND" value={record.device?.toUpperCase() ?? "CPU"} />
						<Meta label="SOURCE" value={record.source} />
						<Meta label="CAPTURE" value={record.capture_device ?? "—"} />
						<Meta
							label="LEVEL"
							value={formatDbfs(record.rms_db)}
							tone={quiet ? "warn" : undefined}
						/>
						{clipped > 0 && <Meta label="CLIPPED" value={formatClipRatio(clipped)} />}
					</div>
				</div>

				{isQuietAndSilent(record) && (
					<div className="flex gap-2.5 px-3.5 py-3 bg-warning-wash border border-warning/30 rounded-lg">
						<AlertTriangle size={15} strokeWidth={1.7} className="text-warning shrink-0 mt-px" />
						<div>
							<div className="text-[12px] font-semibold text-warning">
								Nothing came back, and the input was quiet
							</div>
							<p className="mt-1 text-[11.5px] leading-relaxed text-text-secondary">
								At {formatDbfs(record.rms_db)} this capture is well below a healthy speech level,
								and a capture this quiet is the one that most often returns nothing at all. Check
								the microphone, its gain, and how far away the speaker was — a different model will
								not hear what was not recorded.
							</p>
						</div>
					</div>
				)}
			</div>
		</div>
	);
}
