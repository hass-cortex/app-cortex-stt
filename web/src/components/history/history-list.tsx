import type { HistoryFilters, TranscriptionRecord } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { Spinner } from "@/components/ui/spinner";
import { useHistoryList } from "@/hooks/use-history";
import { formatDuration, formatRelativeTime } from "@/lib/format";
import { AlertCircle, ChevronRight, History, Mic } from "lucide-react";
import { useState } from "react";

const sourceOptions = [
	{ value: "", label: "All sources" },
	{ value: "Wyoming", label: "Wyoming" },
	{ value: "HttpApi", label: "HTTP API" },
];

const errorOptions = [
	{ value: "", label: "All results" },
	{ value: "true", label: "Errors only" },
	{ value: "false", label: "Successful only" },
];

interface HistoryListProps {
	onSelectRecord: (id: string) => void;
}

export function HistoryList({ onSelectRecord }: HistoryListProps) {
	const [filters, setFilters] = useState<HistoryFilters>({ limit: 50 });
	const { data, isLoading, error } = useHistoryList(filters);

	const updateFilter = (key: keyof HistoryFilters, value: string) => {
		setFilters((prev) => ({
			...prev,
			[key]: value === "" ? undefined : value,
			offset: 0,
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

	return (
		<div className="space-y-4">
			{/* Filters */}
			<div className="flex flex-col sm:flex-row gap-3">
				<Select
					options={sourceOptions}
					value={filters.source ?? ""}
					onChange={(e) => updateFilter("source", e.target.value)}
					className="sm:w-36"
				/>
				<Select
					options={errorOptions}
					value={filters.has_error !== undefined ? String(filters.has_error) : ""}
					onChange={(e) => updateFilter("has_error", e.target.value)}
					className="sm:w-40"
				/>
				<Input
					type="text"
					placeholder="Model filter..."
					value={filters.model ?? ""}
					onChange={(e) => updateFilter("model", e.target.value)}
					className="sm:w-40"
				/>
			</div>

			{/* Results */}
			{isLoading ? (
				<div className="flex justify-center py-16">
					<Spinner size="lg" />
				</div>
			) : !data || data.items.length === 0 ? (
				<EmptyState
					icon={<History size={40} />}
					title="No transcriptions yet"
					description="Transcription records will appear here after processing audio."
				/>
			) : (
				<>
					<div className="space-y-1">
						{data.items.map((record) => (
							<HistoryRow
								key={record.id}
								record={record}
								onClick={() => onSelectRecord(record.id)}
							/>
						))}
					</div>

					{/* Pagination */}
					{data.total > (filters.limit ?? 50) && (
						<div className="flex items-center justify-between pt-2">
							<span className="text-xs text-text-muted">
								Showing {(filters.offset ?? 0) + 1}-
								{Math.min((filters.offset ?? 0) + data.items.length, data.total)} of {data.total}
							</span>
							<div className="flex gap-2">
								<Button
									variant="secondary"
									size="sm"
									disabled={(filters.offset ?? 0) === 0}
									onClick={() =>
										setFilters((prev) => ({
											...prev,
											offset: Math.max(0, (prev.offset ?? 0) - (prev.limit ?? 50)),
										}))
									}
								>
									Previous
								</Button>
								<Button
									variant="secondary"
									size="sm"
									disabled={(filters.offset ?? 0) + data.items.length >= data.total}
									onClick={() =>
										setFilters((prev) => ({
											...prev,
											offset: (prev.offset ?? 0) + (prev.limit ?? 50),
										}))
									}
								>
									Next
								</Button>
							</div>
						</div>
					)}
				</>
			)}
		</div>
	);
}

function HistoryRow({
	record,
	onClick,
}: {
	record: TranscriptionRecord;
	onClick: () => void;
}) {
	return (
		<button
			type="button"
			onClick={onClick}
			className="w-full flex items-center gap-3 p-3 rounded-lg hover:bg-surface-2 transition-colors text-left cursor-pointer border border-transparent hover:border-border"
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
					<span className="text-xs text-text-muted">{formatRelativeTime(record.timestamp)}</span>
					<Badge variant={record.source === "Wyoming" ? "info" : "accent"}>{record.source}</Badge>
					<span className="text-xs text-text-muted">{record.model_id}</span>
				</div>
			</div>

			<div className="text-right shrink-0 hidden sm:block">
				<p className="text-xs text-text-secondary">{formatDuration(record.audio_duration_ms)}</p>
				<p className="text-xs text-text-muted">{formatDuration(record.inference_ms)} inference</p>
			</div>

			<ChevronRight size={16} className="text-text-muted shrink-0" />
		</button>
	);
}
