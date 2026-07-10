import { AlertCircle, ChevronDown, ChevronRight, History, Mic, Trash2 } from "lucide-react";
import { useState } from "react";
import type { HistoryFilters, TranscriptionRecord } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { useDeleteHistoryRecord, useHistoryFacets, useHistoryList } from "@/hooks/use-history";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useSettings } from "@/hooks/use-settings";
import { formatDuration } from "@/lib/format";
import { formatTimestamp } from "@/utils/time";
import { AudioPlayer } from "./audio-player";
import { SegmentTimeline } from "./segment-timeline";

const errorOptions = [
	{ value: "", label: "All results" },
	{ value: "true", label: "Errors only" },
	{ value: "false", label: "Successful only" },
];

export function HistoryList() {
	const [filters, setFilters] = useState<HistoryFilters>({ limit: 50 });
	const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
	const { data, isLoading, error } = useHistoryList(filters);
	const { data: facets } = useHistoryFacets();
	const { data: settings } = useSettings();
	const timezone = settings?.timezone ?? "auto";

	const updateFilter = (key: keyof HistoryFilters, value: string) => {
		setFilters((prev) => ({
			...prev,
			[key]: value === "" ? undefined : value,
			offset: 0,
		}));
	};

	const toggleExpand = (id: string) => {
		setExpandedIds((prev) => {
			const next = new Set(prev);
			if (next.has(id)) {
				next.delete(id);
			} else {
				next.add(id);
			}
			return next;
		});
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

	return (
		<div className="space-y-4">
			{/* Filters */}
			<div className="flex flex-col sm:flex-row gap-3">
				<Select
					options={errorOptions}
					value={filters.has_error !== undefined ? String(filters.has_error) : ""}
					onChange={(e) => updateFilter("has_error", e.target.value)}
					className="sm:w-40"
				/>
				<Select
					options={[
						{ value: "", label: "All capture devices" },
						...(facets?.capture_devices ?? []).map((d) => ({ value: d, label: d })),
					]}
					value={filters.capture_device ?? ""}
					onChange={(e) => updateFilter("capture_device", e.target.value)}
					className="sm:w-48"
				/>
				<Select
					options={[
						{ value: "", label: "All models" },
						...(facets?.models ?? []).map((m) => ({ value: m, label: m })),
					]}
					value={filters.model ?? ""}
					onChange={(e) => updateFilter("model", e.target.value)}
					className="sm:w-48"
				/>
				<Input
					type="text"
					placeholder="Search text..."
					value={filters.text ?? ""}
					onChange={(e) => updateFilter("text", e.target.value)}
					className="sm:w-48"
				/>
			</div>

			{/* Results */}
			{isLoading ? (
				<div className="flex justify-center py-16">
					<Spinner size="lg" />
				</div>
			) : records.length === 0 ? (
				<EmptyState
					icon={<History size={40} />}
					title="No transcriptions yet"
					description="Transcription records will appear here after processing audio."
				/>
			) : (
				<div className="space-y-1">
					{expandedIds.size >= 2 && (
						<div className="flex justify-end">
							<Button size="sm" variant="ghost" onClick={() => setExpandedIds(new Set())}>
								Collapse All
							</Button>
						</div>
					)}
					{records.map((record) => (
						<HistoryRow
							key={record.id}
							record={record}
							isExpanded={expandedIds.has(record.id)}
							onToggle={() => toggleExpand(record.id)}
							timezone={timezone}
						/>
					))}
				</div>
			)}
		</div>
	);
}

function HistoryRow({
	record,
	isExpanded,
	onToggle,
	timezone,
}: {
	record: TranscriptionRecord;
	isExpanded: boolean;
	onToggle: () => void;
	timezone: string;
}) {
	return (
		<div
			className={`rounded-lg border transition-colors ${
				isExpanded
					? "border-border bg-surface-1"
					: "border-transparent hover:border-border hover:bg-surface-2"
			}`}
		>
			{/* Summary row */}
			<button
				type="button"
				onClick={onToggle}
				className="w-full flex items-center gap-3 p-3 text-left cursor-pointer"
			>
				<div className="p-1.5 rounded-lg bg-surface-3 shrink-0">
					{record.has_error ? (
						<AlertCircle size={16} className="text-error" />
					) : (
						<Mic size={16} className="text-accent" />
					)}
				</div>

				<div className="flex-1 min-w-0">
					<p className="text-sm text-text-primary truncate">
						{record.text || <span className="text-text-muted italic">Empty</span>}
					</p>
					<div className="flex items-center gap-2 mt-0.5">
						<span className="text-xs text-text-muted">
							{formatTimestamp(record.timestamp, timezone)}
						</span>
						<span className="text-xs text-text-secondary">
							{formatDuration(record.inference_ms)}
						</span>
						<span className="text-xs text-text-muted">{record.model_id}</span>
					</div>
				</div>

				<div className="text-right shrink-0 hidden sm:block">
					<p className="text-xs text-text-muted">{formatDuration(record.audio_duration_ms)}</p>
					<p className="text-xs text-text-secondary">
						{formatDuration(record.inference_ms)} inference
					</p>
					{record.model_load_ms > 0 && (
						<p className="text-xs text-amber-500">{formatDuration(record.model_load_ms)} acquire</p>
					)}
					<p className="text-xs text-text-muted">{record.device?.toUpperCase() ?? "CPU"}</p>
				</div>

				{isExpanded ? (
					<ChevronDown size={16} className="text-text-muted shrink-0" />
				) : (
					<ChevronRight size={16} className="text-text-muted shrink-0" />
				)}
			</button>

			{/* Expanded detail */}
			{isExpanded && <ExpandedDetail record={record} timezone={timezone} />}
		</div>
	);
}

function ExpandedDetail({ record, timezone }: { record: TranscriptionRecord; timezone: string }) {
	const deleteMutation = useDeleteHistoryRecord();
	const runDelete = useMutationToast(deleteMutation, { success: "Record deleted" });

	const segments = record.segments;
	const hasAudio = !!record.audio_path;

	const handleDelete = () => {
		if (!window.confirm("Delete this transcription record?")) return;
		runDelete(record.id);
	};

	return (
		<div className="px-3 pb-3 space-y-3 border-t border-border">
			{/* Metadata grid */}
			<div className="grid grid-cols-2 sm:grid-cols-4 gap-3 pt-3 text-xs">
				<div>
					<span className="text-text-muted">Timestamp</span>
					<p className="text-text-primary">{formatTimestamp(record.timestamp, timezone)}</p>
				</div>
				<div>
					<span className="text-text-muted">Audio Duration</span>
					<p className="text-text-primary">{formatDuration(record.audio_duration_ms)}</p>
				</div>
				<div>
					<span className="text-text-muted">Inference Time</span>
					<p className="text-text-primary">{formatDuration(record.inference_ms)}</p>
				</div>
				{record.model_load_ms > 0 && (
					<div>
						<span className="text-text-muted">Acquire Time</span>
						<p className="text-amber-500">{formatDuration(record.model_load_ms)}</p>
					</div>
				)}
				{record.pool_wait_ms > 0 && (
					<div>
						<span className="text-text-muted">Pool Wait</span>
						<p className="text-text-primary">{formatDuration(record.pool_wait_ms)}</p>
					</div>
				)}
				{record.cold_load_ms > 0 && (
					<div>
						<span className="text-text-muted">Cold Load</span>
						<p className="text-amber-500">{formatDuration(record.cold_load_ms)}</p>
					</div>
				)}
				<div>
					<span className="text-text-muted">RTF</span>
					<p className="text-text-primary">
						{record.audio_duration_ms > 0
							? (record.inference_ms / record.audio_duration_ms).toFixed(2)
							: "N/A"}
						x
					</p>
				</div>
				<div>
					<span className="text-text-muted">Device</span>
					<p className="text-text-primary">
						<Badge variant={record.device === "cuda" ? "info" : "default"}>
							{record.device?.toUpperCase() ?? "CPU"}
						</Badge>
					</p>
				</div>
				{record.capture_device && (
					<div>
						<span className="text-text-muted">Capture Device</span>
						<p className="text-text-primary">{record.capture_device}</p>
					</div>
				)}
				{record.rms_db != null && (
					<div>
						<span className="text-text-muted">Audio Level</span>
						<p
							className={
								record.rms_db < -40 || (record.clip_ratio ?? 0) > 0.01
									? "text-amber-500"
									: "text-text-primary"
							}
						>
							{record.rms_db.toFixed(1)} dBFS
							{(record.clip_ratio ?? 0) > 0 &&
								` · clip ${((record.clip_ratio ?? 0) * 100).toFixed(1)}%`}
						</p>
					</div>
				)}
			</div>

			{/* Error message */}
			{record.has_error && record.error_message && (
				<div className="p-3 bg-error/10 border border-error/30 rounded-lg">
					<p className="text-sm text-error">{record.error_message}</p>
				</div>
			)}

			{/* Transcript text */}
			<div className="p-3 bg-surface-3 rounded-lg select-text">
				<p className="text-sm text-text-primary whitespace-pre-wrap cursor-text">
					{record.text || <span className="text-text-muted italic">Empty transcript</span>}
				</p>
			</div>

			{/* Audio player */}
			{hasAudio && <AudioPlayer recordId={record.id} durationMs={record.audio_duration_ms} />}

			{/* Segments */}
			<SegmentTimeline segments={segments} totalDurationMs={record.audio_duration_ms} />

			{/* Delete button */}
			<div className="flex justify-end">
				<Button
					variant="danger"
					size="sm"
					icon={<Trash2 size={14} />}
					onClick={handleDelete}
					loading={deleteMutation.isPending}
				>
					Delete
				</Button>
			</div>
		</div>
	);
}
