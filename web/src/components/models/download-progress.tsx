import { ProgressBar } from "@/components/ui/progress-bar";
import { useCancelDownload, useDownloadProgress } from "@/hooks/use-models";
import { formatBytes, formatDuration } from "@/lib/format";
import { X } from "lucide-react";

interface DownloadProgressProps {
	modelId: string;
}

export function DownloadProgressBar({ modelId }: DownloadProgressProps) {
	const progress = useDownloadProgress(modelId);
	const cancelMutation = useCancelDownload();

	if (!progress) return null;

	const percent =
		progress.total_bytes > 0
			? Math.round((progress.downloaded_bytes / progress.total_bytes) * 100)
			: 0;

	const statusLabel =
		progress.status === "downloading"
			? `${formatBytes(progress.downloaded_bytes)} / ${formatBytes(progress.total_bytes)}`
			: progress.status === "verifying"
				? "Verifying SHA256..."
				: progress.status === "extracting"
					? "Extracting..."
					: progress.status === "completed"
						? "Complete"
						: `Error: ${progress.error}`;

	const variant =
		progress.status === "failed" ? "error" : progress.status === "completed" ? "success" : "accent";

	return (
		<div className="space-y-1.5">
			<ProgressBar value={percent} variant={variant} size="md" />
			<div className="flex items-center justify-between text-xs">
				<span className="text-text-muted">{statusLabel}</span>
				<div className="flex items-center gap-2">
					{progress.status === "downloading" && (
						<>
							<span className="text-text-muted">{formatBytes(progress.speed_bps)}/s</span>
							{progress.eta_secs != null && (
								<span className="text-text-muted">
									ETA {formatDuration(progress.eta_secs * 1000)}
								</span>
							)}
						</>
					)}
					{(progress.status === "downloading" || progress.status === "verifying") && (
						<button
							type="button"
							onClick={() => cancelMutation.mutate(modelId)}
							className="p-0.5 text-text-muted hover:text-error transition-colors cursor-pointer"
							title="Cancel download"
						>
							<X size={14} />
						</button>
					)}
				</div>
			</div>
		</div>
	);
}
