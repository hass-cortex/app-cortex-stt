import { Download, Info, X } from "lucide-react";
import { useState } from "react";
import type { ModelInfo } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Hint } from "@/components/ui/hint";
import { useCancelDownload, useDownloadModel } from "@/hooks/use-models";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { formatMB } from "@/lib/format";
import { variantName } from "@/lib/variant";
import { DownloadProgressBar } from "./download-progress";

const cell = "num text-[11.5px] text-text-secondary";

export function CatalogHead() {
	const head = "num text-[10px] tracking-[0.06em] text-text-faint";
	return (
		<div className="hidden lg:flex items-center gap-3 px-3.5 pb-1.5">
			<span className={`${head} flex-1`}>MODEL</span>
			<span className={`${head} w-24`}>FAMILY</span>
			<span className={`${head} w-[42px] text-right hidden xl:block`}>LANG</span>
			<span className={`${head} w-[150px]`}>QUANT</span>
			<span className={`${head} w-[72px] text-right`}>SIZE</span>
			<span className="w-[110px]" />
		</div>
	);
}

/** A model with nothing on disk yet: pick the quant, then download it. */
export function ModelRow({ model }: { model: ModelInfo }) {
	const downloadMutation = useDownloadModel();
	const cancelMutation = useCancelDownload();
	const runDownload = useMutationToast(downloadMutation, {
		success: `Downloading ${model.name}…`,
		error: "Download failed",
	});
	const runCancel = useMutationToast(cancelMutation, {
		success: `${model.name} removed from the queue`,
		error: "Cancel failed",
	});

	const inFlight = model.variants.filter(
		(v) => v.status === "downloading" || v.status === "queued",
	);
	const failed = model.variants.filter((v) => v.download_error && !inFlight.includes(v));
	const [quant, setQuant] = useState(model.default_quant);
	const queued = model.variants.some((v) => v.status === "queued");

	return (
		<div className="px-3.5 py-2.5 lg:py-2 border-b border-border-soft last:border-0 lg:border-0">
			<div className="flex flex-wrap items-center gap-x-3 gap-y-2 lg:flex-nowrap">
				<div className="flex items-center gap-2.5 min-w-0 basis-full lg:basis-auto lg:flex-1">
					<div className="flex flex-col gap-0.5 min-w-0">
						<span className="text-[14px] lg:text-[12.5px] text-text-primary truncate">
							{model.name}
						</span>
						<span className="lg:hidden num text-[11px] text-text-muted truncate">
							{model.family}
							{model.languages.length > 0 &&
								` · ${model.languages.length} ${model.languages.length === 1 ? "language" : "languages"}`}
						</span>
					</div>
					{model.description && (
						<Hint
							label={model.name}
							content={model.description}
							className="shrink-0 text-text-faint hover:text-text-secondary"
						>
							<Info size={12} strokeWidth={1.8} />
						</Hint>
					)}
					{model.status === "error" && <Badge variant="error">error</Badge>}
				</div>
				<span className={`${cell} hidden lg:block w-24 truncate`}>{model.family}</span>
				<span className={`${cell} w-[42px] text-right hidden xl:block`}>
					{model.languages.length}
				</span>

				{inFlight.length === 0 ? (
					<>
						<select
							value={quant}
							onChange={(e) => setQuant(e.target.value)}
							aria-label={`Quant of ${model.name} to download`}
							className="num flex-1 min-w-0 lg:flex-none lg:w-[150px] h-11 lg:h-8 px-2 bg-surface-3 border border-border rounded-[7px] text-[11.5px] text-text-primary cursor-pointer"
						>
							{model.quants.map((q) => (
								<option key={q.quant} value={q.quant}>
									{q.quant}
									{q.quant === model.default_quant ? " · default" : ""}
								</option>
							))}
						</select>
						<span className={`${cell} lg:w-[72px] lg:text-right whitespace-nowrap`}>
							{formatMB(model.quants.find((q) => q.quant === quant)?.size_mb ?? model.size_mb)}
						</span>
						<div className="flex justify-end lg:w-[110px]">
							<Button
								size="sm"
								className="max-lg:h-11 max-lg:px-4 max-lg:text-[13.5px]"
								icon={<Download size={13} strokeWidth={1.8} />}
								onClick={() => runDownload({ modelId: model.id, quant })}
								loading={downloadMutation.isPending}
							>
								Download
							</Button>
						</div>
					</>
				) : (
					<div className="flex justify-end ml-auto">
						{queued && (
							<Button
								size="sm"
								variant="ghost"
								icon={<X size={13} strokeWidth={1.8} />}
								onClick={() => runCancel({ modelId: model.id, quant: inFlight[0]?.quant ?? null })}
								loading={cancelMutation.isPending}
							>
								Cancel
							</Button>
						)}
					</div>
				)}
			</div>

			{inFlight.map((v) => (
				<div key={v.quant ?? "custom"} className="mt-2 flex items-start gap-2">
					<span className="num text-[10.5px] text-text-muted w-14 shrink-0 pt-0.5">{v.quant}</span>
					<div className="flex-1 min-w-0">
						<DownloadProgressBar modelId={model.id} quant={v.quant} />
					</div>
				</div>
			))}

			{failed.map((v) => (
				<div key={v.quant ?? "custom"} className="mt-2 flex items-start gap-1.5 text-xs text-error">
					<span className="min-w-0 break-words">
						{variantName(v.quant)} download failed: {v.download_error}
					</span>
					<button
						type="button"
						aria-label="Dismiss download error"
						onClick={() => cancelMutation.mutate({ modelId: model.id, quant: v.quant })}
						className="shrink-0 p-0.5 text-text-muted hover:text-text-primary transition-colors cursor-pointer"
					>
						<X size={12} />
					</button>
				</div>
			))}
		</div>
	);
}
