import type { ModelInfo } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useToast } from "@/components/ui/toast";
import { useDeleteModel, useDownloadModel } from "@/hooks/use-models";
import { formatMB } from "@/lib/format";
import { Download, Trash2 } from "lucide-react";
import { DownloadProgressBar } from "./download-progress";
import { ScoreBar } from "./score-bar";

interface ModelCardProps {
	model: ModelInfo;
}

export function ModelCard({ model }: ModelCardProps) {
	const { toast } = useToast();
	const downloadMutation = useDownloadModel();
	const deleteMutation = useDeleteModel();

	const isDownloading = model.status === "downloading";
	const isDownloaded =
		model.status === "downloaded" || model.status === "loaded" || model.status === "loading";
	const isLoaded = model.status === "loaded";

	return (
		<Card className="flex flex-col">
			{/* Header */}
			<div className="flex items-start justify-between mb-3">
				<div className="min-w-0">
					<div className="flex items-center gap-2">
						<h3 className="text-sm font-semibold text-text-primary truncate">{model.name}</h3>
						{model.is_recommended && <Badge variant="accent">Recommended</Badge>}
					</div>
					<p className="text-xs text-text-muted mt-0.5">{model.description}</p>
				</div>
			</div>

			{/* Tags */}
			<div className="flex flex-wrap gap-1.5 mb-3">
				<Badge>{model.engine_type}</Badge>
				<Badge>{formatMB(model.size_mb)}</Badge>
				{model.uses_gpu && <Badge variant="accent">GPU</Badge>}
				{!model.uses_gpu && <Badge variant="default">CPU</Badge>}
				{model.requires_avx && <Badge variant="warning">AVX</Badge>}
				{isLoaded && <Badge variant="success">Loaded</Badge>}
				{model.status === "error" && <Badge variant="error">Error</Badge>}
			</div>

			{/* Scores */}
			<div className="space-y-2 mb-3">
				<ScoreBar label="Accuracy" score={model.accuracy_score} variant="success" />
				<ScoreBar label="Speed" score={model.speed_score} variant="accent" />
			</div>

			{/* Languages */}
			<p
				className="text-xs text-text-muted mb-3 cursor-default"
				title={model.supported_languages.join(", ")}
			>
				{model.supported_languages.length > 5
					? `${model.supported_languages.length} languages: ${model.supported_languages.slice(0, 5).join(", ")}…`
					: model.supported_languages.join(", ")}
			</p>

			{/* Download progress */}
			{isDownloading && (
				<div className="mb-3">
					<DownloadProgressBar modelId={model.id} />
				</div>
			)}

			{/* Actions */}
			<div className="flex items-center gap-2 mt-auto pt-3 border-t border-border">
				{!isDownloaded && !isDownloading && model.status !== "custom" && (
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
				)}
				{isDownloaded && (
					<Button
						size="sm"
						variant="danger"
						icon={<Trash2 size={14} />}
						onClick={() => {
							if (window.confirm(`Delete ${model.name}? This cannot be undone.`)) {
								deleteMutation.mutate(model.id);
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
