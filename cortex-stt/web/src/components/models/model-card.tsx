import type { ModelInfo, SystemInfo } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useToast } from "@/components/ui/toast";
import { useCancelDownload, useDeleteModel, useDownloadModel } from "@/hooks/use-models";
import { formatMB } from "@/lib/format";
import { Clock, Download, Trash2, X } from "lucide-react";
import { useState } from "react";
import { DownloadProgressBar } from "./download-progress";
import { ScoreBar } from "./score-bar";

interface ModelCardProps {
	model: ModelInfo;
	systemInfo?: SystemInfo;
}

export function ModelCard({ model, systemInfo }: ModelCardProps) {
	const { toast } = useToast();
	const downloadMutation = useDownloadModel();
	const deleteMutation = useDeleteModel();
	const cancelMutation = useCancelDownload();

	const [showAllLangs, setShowAllLangs] = useState(false);

	const incompatibleReasons: string[] = [];
	if (systemInfo) {
		if (model.requires_avx && !systemInfo.has_avx) incompatibleReasons.push("Requires AVX");
		if (model.requires_cuda && !(systemInfo.cuda_available && systemInfo.gpu_engines?.whisper))
			incompatibleReasons.push("Requires CUDA");
	}
	const isIncompatible = incompatibleReasons.length > 0;

	const isQueued = model.status === "queued";
	const isDownloading = model.status === "downloading";
	const isDownloaded =
		model.status === "downloaded" || model.status === "loaded" || model.status === "loading";
	const isLoaded = model.status === "loaded";

	return (
		<Card className="flex flex-col">
			{/* Header */}
			<div className="flex items-start justify-between mb-3">
				<div className="min-w-0">
					<h3 className="text-sm font-semibold text-text-primary truncate">{model.name}</h3>
					<p className="text-xs text-text-muted mt-0.5">{model.description}</p>
				</div>
			</div>

			{/* Tags */}
			<div className="flex flex-wrap gap-1.5 mb-3">
				<Badge>{model.engine_type}</Badge>
				<Badge>{formatMB(model.size_mb)}</Badge>
				{model.uses_gpu && <Badge variant="accent">GPU</Badge>}
				{!model.uses_gpu && <Badge variant="default">CPU</Badge>}
				{isLoaded && <Badge variant="success">Loaded</Badge>}
				{model.status === "error" && <Badge variant="error">Error</Badge>}
			</div>

			{/* Scores */}
			<div className="space-y-2 mb-3">
				<ScoreBar label="Accuracy" score={model.accuracy_score} variant="success" />
				<ScoreBar label="Speed" score={model.speed_score} variant="accent" />
			</div>

			{/* Languages */}
			<div className="text-xs text-text-muted mb-3">
				{model.supported_languages.length <= 5 ? (
					<span>{model.supported_languages.join(", ")}</span>
				) : showAllLangs ? (
					<>
						<span>{model.supported_languages.join(", ")}</span>
						<button
							type="button"
							className="ml-1 text-accent-primary hover:underline"
							onClick={() => setShowAllLangs(false)}
						>
							less
						</button>
					</>
				) : (
					<>
						<span>{model.supported_languages.slice(0, 5).join(", ")}</span>
						<button
							type="button"
							className="ml-1 text-accent-primary hover:underline"
							onClick={() => setShowAllLangs(true)}
						>
							+{model.supported_languages.length - 5} more
						</button>
					</>
				)}
			</div>

			{/* Download progress */}
			{isDownloading && (
				<div className="mb-3">
					<DownloadProgressBar modelId={model.id} />
				</div>
			)}

			{/* Queued indicator */}
			{isQueued && (
				<div className="flex items-center gap-2 mb-3 text-xs text-text-muted">
					<Clock size={14} className="animate-pulse" />
					<span>Queued — waiting for other downloads to finish</span>
				</div>
			)}

			{/* Actions */}
			<div className="flex items-center gap-2 mt-auto pt-3 border-t border-border">
				{!isDownloaded && !isDownloading && !isQueued && model.status !== "custom" && (
					isIncompatible ? (
						<span className="text-xs text-text-muted">{incompatibleReasons.join(", ")}</span>
					) : (
						<Button
							size="sm"
							icon={<Download size={14} />}
							onClick={() =>
								downloadMutation.mutate(model.id, {
									onSuccess: () => toast(`Downloading ${model.name}...`, "success"),
									onError: (err) => toast(`Download failed: ${err.message}`, "error"),
								})
							}
							loading={downloadMutation.isPending}
						>
							Download
						</Button>
					)
				)}
				{isQueued && (
					<Button
						size="sm"
						variant="ghost"
						icon={<X size={14} />}
						onClick={() =>
							cancelMutation.mutate(model.id, {
								onSuccess: () => toast(`${model.name} removed from queue`, "success"),
								onError: (err) => toast(`Cancel failed: ${err.message}`, "error"),
							})
						}
						loading={cancelMutation.isPending}
					>
						Cancel
					</Button>
				)}
				{isDownloaded && (
					<Button
						size="sm"
						variant="danger"
						icon={<Trash2 size={14} />}
						onClick={() => {
							if (window.confirm(`Delete ${model.name}? This cannot be undone.`)) {
								deleteMutation.mutate(model.id, {
									onSuccess: () => toast(`${model.name} deleted`, "success"),
									onError: (err) => toast(`Delete failed: ${err.message}`, "error"),
								});
							}
						}}
						loading={deleteMutation.isPending}
						disabled={isLoaded}
					>
						Delete
					</Button>
				)}
			</div>
		</Card>
	);
}
