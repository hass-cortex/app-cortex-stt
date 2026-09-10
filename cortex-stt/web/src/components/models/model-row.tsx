import { Download, FlaskConical, Info, Play, Power, Trash2, X } from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import type { ModelInfo } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useConfirm } from "@/components/ui/confirm";
import { Hint } from "@/components/ui/hint";
import { useLoadModel, useUnloadModel } from "@/hooks/use-engine";
import { useCancelDownload, useDeleteModel, useDownloadModel } from "@/hooks/use-models";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { ROUTES } from "@/lib/constants";
import { formatDuration, formatMB } from "@/lib/format";
import type { ModelLatency } from "@/lib/stats";
import { DownloadProgressBar } from "./download-progress";

interface ModelRowProps {
	model: ModelInfo;
	/** Measured here, over the last week. Absent means "never run" — never
	 *  a catalog claim standing in for a measurement. */
	measured?: ModelLatency;
	isDefault?: boolean;
}

const cell = "num text-[11.5px] text-text-secondary";

/** The download API keys on the quant NAME, so two catalog entries sharing
 *  one are not two choices — offering both duplicates a React key and
 *  cannot change what gets fetched. */
function uniqueQuants(quants: ModelInfo["quants"]): ModelInfo["quants"] {
	const seen = new Set<string>();
	return quants.filter((q) => {
		if (seen.has(q.quant)) return false;
		seen.add(q.quant);
		return true;
	});
}

export function ModelRow({ model, measured, isDefault }: ModelRowProps) {
	const downloadMutation = useDownloadModel();
	const deleteMutation = useDeleteModel();
	const cancelMutation = useCancelDownload();
	const loadMutation = useLoadModel();
	const unloadMutation = useUnloadModel();

	const runDownload = useMutationToast(downloadMutation, {
		success: `Downloading ${model.name}…`,
		error: "Download failed",
	});
	const runCancel = useMutationToast(cancelMutation, {
		success: `${model.name} removed from the queue`,
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

	const confirm = useConfirm();
	const [quant, setQuant] = useState(model.default_quant);

	const DeleteButton = () => (
		<button
			type="button"
			aria-label={`Delete ${model.name}`}
			className="inline-flex items-center px-1.5 py-1 rounded-md text-text-muted hover:bg-error-wash hover:text-error transition-colors cursor-pointer"
			onClick={async () => {
				const ok = await confirm({
					title: `Delete ${model.name}?`,
					body: `The ${model.downloaded_quant ?? ""} file leaves the disk and has to be downloaded again to come back. Evaluation results already recorded for it are kept.`,
					confirmLabel: "Delete model",
					destructive: true,
				});
				if (ok) runDelete(model.id);
			}}
		>
			<Trash2 size={13} strokeWidth={1.8} />
		</button>
	);

	const isQueued = model.status === "queued";
	const isDownloading = model.status === "downloading";
	const onDisk = model.status === "downloaded" || model.status === "custom";
	const loaded = model.is_loaded;

	return (
		<div
			className={`px-3.5 py-2.5 lg:py-2 border-b border-border-soft last:border-0 lg:border-0 rounded-lg ${
				loaded ? "bg-accent-wash shadow-[inset_2px_0_0_var(--accent)]" : ""
			}`}
		>
			{/* Below lg the fixed-width metric columns leave the name nothing
			    to occupy, so the row stacks: identity, then metrics, then
			    actions. From lg up it is a table row again. */}
			<div className="flex flex-col gap-2 lg:flex-row lg:items-center lg:gap-3">
				<div className="flex items-center gap-3 min-w-0 lg:flex-1">
					<span
						role="img"
						aria-label={loaded ? "resident" : "on disk, not loaded"}
						className={`w-[9px] h-[9px] rounded-full shrink-0 ${
							loaded ? "bg-success" : "border border-border"
						}`}
					/>
					<span
						className={`text-[12.5px] truncate ${
							loaded ? "font-semibold text-text-primary" : "text-text-primary"
						}`}
					>
						{model.name}
					</span>
					{model.description && (
						<Hint
							label={model.name}
							content={model.description}
							className="shrink-0 text-text-faint hover:text-text-secondary"
						>
							<Info size={12} strokeWidth={1.8} />
						</Hint>
					)}
					{isDefault && <Badge variant="accent">default</Badge>}
					{model.status === "error" && <Badge variant="error">error</Badge>}
				</div>

				<div className="flex items-center gap-3 pl-[21px] lg:pl-0 lg:contents">
					<span className={`${cell} w-24 hidden lg:block truncate`}>{model.family}</span>
					<span className={`${cell} hidden sm:block lg:hidden truncate`}>{model.family}</span>

					{onDisk ? (
						<span className={`${cell} w-[62px] hidden lg:block`}>{model.downloaded_quant}</span>
					) : (
						<select
							value={quant}
							onChange={(e) => setQuant(e.target.value)}
							disabled={uniqueQuants(model.quants).length <= 1}
							className="num w-[62px] h-6 px-1.5 hidden lg:block bg-surface-3 border border-border rounded-[5px] text-[10.5px] text-text-secondary cursor-pointer"
						>
							{uniqueQuants(model.quants).map((q) => (
								<option key={q.quant} value={q.quant}>
									{q.quant}
								</option>
							))}
						</select>
					)}

					<span className={`${cell} lg:w-[62px] lg:text-right whitespace-nowrap min-w-0 truncate`}>
						{formatMB(
							onDisk
								? model.size_mb
								: (model.quants.find((q) => q.quant === quant)?.size_mb ?? model.size_mb),
						)}
					</span>
					<span className={`${cell} w-[42px] text-right hidden xl:block`}>
						{model.languages.length}
					</span>
					<span
						className={`num lg:w-24 lg:text-right text-[11.5px] whitespace-nowrap min-w-0 truncate ${
							measured ? "text-text-primary" : "text-text-faint"
						}`}
					>
						{/* Without the column header above it, a bare duration on a
						    phone does not say which duration it is. */}
						<span className="lg:hidden text-text-faint">p50 </span>
						{measured ? formatDuration(measured.p50) : "—"}
					</span>
					<span className={`${cell} w-12 text-right hidden xl:block`}>
						{measured ? measured.runs : "—"}
					</span>

					{/* Fixed slots, not a right-aligned run: a loaded model has no
				    delete button, and without a reserved slot its Unload would sit
				    one position further right than every Load below it. A row with
				    no file on disk has neither icon, and the Catalog section is
				    entirely such rows — reserving the slots there would only push
				    every Download button off the right edge the list aligns on. */}
					<div className="flex justify-end items-center gap-1.5 shrink-0 ml-auto lg:ml-0 w-[146px] lg:w-[190px]">
						<div className="flex justify-end w-[86px] lg:w-[104px]">
							{!onDisk && !isDownloading && !isQueued && (
								<Button
									size="sm"
									variant="secondary"
									icon={<Download size={13} strokeWidth={1.8} />}
									onClick={() => runDownload({ modelId: model.id, quant })}
									loading={downloadMutation.isPending}
								>
									Download
								</Button>
							)}
							{isQueued && (
								<Button
									size="sm"
									variant="ghost"
									icon={<X size={13} strokeWidth={1.8} />}
									onClick={() => runCancel(model.id)}
									loading={cancelMutation.isPending}
								>
									Cancel
								</Button>
							)}
							{onDisk && !loaded && (
								<Button
									size="sm"
									variant="ghost"
									icon={<Play size={13} strokeWidth={1.8} />}
									onClick={() => runLoad({ modelId: model.id })}
									loading={loadMutation.isPending}
								>
									Load
								</Button>
							)}
							{loaded && (
								<Button
									size="sm"
									variant="outline"
									icon={<Power size={13} strokeWidth={1.8} />}
									onClick={() => runUnload(model.id)}
									loading={unloadMutation.isPending}
								>
									Unload
								</Button>
							)}
						</div>

						{onDisk && (
							<>
								<div className="flex justify-end w-6 lg:w-7">
									<Link
										to={`${ROUTES.EVAL}?model=${encodeURIComponent(model.id)}`}
										aria-label="Evaluation runs for this model"
										className="inline-flex items-center px-1.5 py-1 rounded-md text-text-muted hover:bg-surface-3 hover:text-text-primary transition-colors"
									>
										<FlaskConical size={13} strokeWidth={1.8} />
									</Link>
								</div>

								<div className="flex justify-end w-6 lg:w-7">{!loaded && <DeleteButton />}</div>
							</>
						)}
					</div>
				</div>
			</div>

			{isDownloading && (
				<div className="mt-2 pl-6">
					<DownloadProgressBar modelId={model.id} />
				</div>
			)}
		</div>
	);
}
