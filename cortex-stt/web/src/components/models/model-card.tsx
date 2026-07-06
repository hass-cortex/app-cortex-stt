import { Clock, Download, Play, PowerOff, Star, Trash2, X } from "lucide-react";
import { useState } from "react";
import type { ModelInfo } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { useLoadModel, useUnloadModel } from "@/hooks/use-engine";
import { useCancelDownload, useDeleteModel, useDownloadModel } from "@/hooks/use-models";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { formatMB } from "@/lib/format";
import { DownloadProgressBar } from "./download-progress";
import { ScoreBar } from "./score-bar";

interface ModelCardProps {
	model: ModelInfo;
}

/** Title-case a family slug for display (e.g. "sensevoice" → "Sensevoice"). */
function familyLabel(family: string): string {
	return family.charAt(0).toUpperCase() + family.slice(1);
}

export function ModelCard({ model }: ModelCardProps) {
	const downloadMutation = useDownloadModel();
	const deleteMutation = useDeleteModel();
	const cancelMutation = useCancelDownload();
	const loadMutation = useLoadModel();
	const unloadMutation = useUnloadModel();

	const runDownload = useMutationToast(downloadMutation, {
		success: `Downloading ${model.name}...`,
		error: "Download failed",
	});
	const runCancel = useMutationToast(cancelMutation, {
		success: `${model.name} removed from queue`,
		error: "Cancel failed",
	});
	const runLoad = useMutationToast(loadMutation, {
		success: `${model.name} loaded`,
		error: "Load failed",
	});
	const runUnload = useMutationToast(unloadMutation, {
		success: `${model.name} unloaded`,
		error: "Unload failed",
	});
	const runDelete = useMutationToast(deleteMutation, {
		success: `${model.name} deleted`,
		error: "Delete failed",
	});

	const [showAllLangs, setShowAllLangs] = useState(false);
	const [selectedQuant, setSelectedQuant] = useState(model.default_quant);

	const hasQuantChoice = model.quants.length > 1;

	const isQueued = model.status === "queued";
	const isDownloading = model.status === "downloading";
	const isDownloaded = model.status === "downloaded" || model.status === "custom";
	const isLoaded = model.is_loaded;

	return (
		<Card className={`flex flex-col ${isLoaded ? "border-2 border-success" : ""}`}>
			{/* Header */}
			<div className="flex items-start justify-between mb-3">
				<div className="min-w-0">
					<h3 className="text-sm font-semibold text-text-primary truncate">{model.name}</h3>
					<p className="text-xs text-text-muted mt-0.5">{model.description}</p>
				</div>
			</div>

			{/* Tags */}
			<div className="flex flex-wrap gap-1.5 mb-3">
				<Badge>{familyLabel(model.family)}</Badge>
				<Badge>{formatMB(model.size_mb)}</Badge>
				{model.recommended && (
					<Badge variant="accent" className="gap-1">
						<Star size={11} />
						Recommended
					</Badge>
				)}
				{model.capabilities.streaming && <Badge variant="info">streaming</Badge>}
				{model.capabilities.translate && <Badge variant="info">translate</Badge>}
				{isDownloaded && model.downloaded_quant && (
					<Badge variant="success">{model.downloaded_quant}</Badge>
				)}
				{model.status === "error" && <Badge variant="error">Error</Badge>}
			</div>

			{/* Scores */}
			<div className="space-y-2 mb-3">
				<ScoreBar label="Accuracy" score={model.accuracy_score} variant="success" />
				<ScoreBar label="Speed" score={model.speed_score} variant="accent" />
			</div>

			{/* Languages */}
			<div className="text-xs text-text-muted mb-3">
				{model.languages.length <= 5 ? (
					<span>{model.languages.join(", ")}</span>
				) : showAllLangs ? (
					<>
						<span>{model.languages.join(", ")}</span>
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
						<span>{model.languages.slice(0, 5).join(", ")}</span>
						<button
							type="button"
							className="ml-1 text-accent-primary hover:underline"
							onClick={() => setShowAllLangs(true)}
						>
							+{model.languages.length - 5} more
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
				{!isDownloaded && !isDownloading && !isQueued && (
					<>
						{hasQuantChoice && (
							<Select
								options={model.quants.map((q) => ({
									value: q.quant,
									label: `${q.quant} · ${formatMB(q.size_mb)}`,
								}))}
								value={selectedQuant}
								onChange={(e) => setSelectedQuant(e.target.value)}
								className="w-36"
							/>
						)}
						<Button
							size="sm"
							icon={<Download size={14} />}
							onClick={() =>
								runDownload({
									modelId: model.id,
									quant: hasQuantChoice ? selectedQuant : undefined,
								})
							}
							loading={downloadMutation.isPending}
						>
							Download
						</Button>
					</>
				)}
				{isQueued && (
					<Button
						size="sm"
						variant="ghost"
						icon={<X size={14} />}
						onClick={() => runCancel(model.id)}
						loading={cancelMutation.isPending}
					>
						Cancel
					</Button>
				)}
				{isDownloaded && !isLoaded && (
					<Button
						size="sm"
						variant="ghost"
						icon={<Play size={14} />}
						onClick={() => runLoad({ modelId: model.id })}
						loading={loadMutation.isPending}
					>
						Load
					</Button>
				)}
				{isLoaded && (
					<Button
						size="sm"
						variant="ghost"
						icon={<PowerOff size={14} />}
						onClick={() => runUnload(model.id)}
						loading={unloadMutation.isPending}
					>
						Unload
					</Button>
				)}
				{isDownloaded && !isLoaded && (
					<Button
						size="sm"
						variant="danger"
						icon={<Trash2 size={14} />}
						onClick={() => {
							if (window.confirm(`Delete ${model.name}? This cannot be undone.`)) {
								runDelete(model.id);
							}
						}}
						loading={deleteMutation.isPending}
					>
						Delete
					</Button>
				)}
			</div>
		</Card>
	);
}
