import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useToast } from "@/components/ui/toast";
import { useDeleteHistoryRecord, useHistoryDetail } from "@/hooks/use-history";
import { formatDuration, formatTimestamp } from "@/lib/format";
import { ArrowLeft, Trash2 } from "lucide-react";
import { AudioPlayer } from "./audio-player";
import { SegmentTimeline } from "./segment-timeline";

interface HistoryDetailProps {
	recordId: string;
	onBack: () => void;
}

export function HistoryDetail({ recordId, onBack }: HistoryDetailProps) {
	const { data: record, isLoading } = useHistoryDetail(recordId);
	const deleteMutation = useDeleteHistoryRecord();
	const { toast } = useToast();

	if (isLoading) {
		return (
			<div className="flex justify-center py-16">
				<Spinner size="lg" />
			</div>
		);
	}

	if (!record) return null;

	const handleDelete = () => {
		if (!window.confirm("Delete this transcription record?")) return;
		deleteMutation.mutate(recordId, {
			onSuccess: () => {
				toast("Record deleted", "success");
				onBack();
			},
			onError: (err) => toast(`Failed: ${err.message}`, "error"),
		});
	};

	return (
		<div className="space-y-4">
			<div className="flex items-center gap-3">
				<Button variant="ghost" size="sm" icon={<ArrowLeft size={16} />} onClick={onBack}>
					Back
				</Button>
			</div>

			<Card>
				<CardHeader
					title="Transcription Detail"
					action={
						<Button
							variant="danger"
							size="sm"
							icon={<Trash2 size={14} />}
							onClick={handleDelete}
							loading={deleteMutation.isPending}
						>
							Delete
						</Button>
					}
				/>

				{/* Metadata */}
				<div className="flex flex-wrap gap-2 mb-4">
					<Badge variant={record.source === "Wyoming" ? "info" : "accent"}>{record.source}</Badge>
					<Badge>{record.model_id}</Badge>
					{record.language && <Badge>{record.language}</Badge>}
					{record.has_error && <Badge variant="error">Error</Badge>}
				</div>

				<div className="grid grid-cols-2 sm:grid-cols-4 gap-3 mb-4 text-xs">
					<div>
						<span className="text-text-muted">Timestamp</span>
						<p className="text-text-primary">{formatTimestamp(record.timestamp)}</p>
					</div>
					<div>
						<span className="text-text-muted">Audio Duration</span>
						<p className="text-text-primary">{formatDuration(record.audio_duration_ms)}</p>
					</div>
					<div>
						<span className="text-text-muted">Inference Time</span>
						<p className="text-text-primary">{formatDuration(record.inference_ms)}</p>
					</div>
					<div>
						<span className="text-text-muted">RTF</span>
						<p className="text-text-primary">
							{record.audio_duration_ms > 0
								? (record.inference_ms / record.audio_duration_ms).toFixed(2)
								: "N/A"}
							x
						</p>
					</div>
				</div>

				{/* Error message */}
				{record.has_error && record.error_message && (
					<div className="mb-4 p-3 bg-error/10 border border-error/30 rounded-lg">
						<p className="text-sm text-error">{record.error_message}</p>
					</div>
				)}

				{/* Transcript text */}
				<div className="mb-4 p-3 bg-surface-3 rounded-lg">
					<p className="text-sm text-text-primary whitespace-pre-wrap">
						{record.text || <span className="text-text-muted italic">Empty transcript</span>}
					</p>
				</div>

				{/* Audio player */}
				{record.has_audio && (
					<div className="mb-4">
						<AudioPlayer recordId={record.id} durationMs={record.audio_duration_ms} />
					</div>
				)}

				{/* Segments */}
				<SegmentTimeline segments={record.segments} totalDurationMs={record.audio_duration_ms} />
			</Card>
		</div>
	);
}
