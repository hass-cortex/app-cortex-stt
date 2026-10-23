import {
	Download,
	FlaskConical,
	Info,
	Play,
	Plus,
	Power,
	RefreshCw,
	Star,
	Trash2,
	X,
} from "lucide-react";
import { useState } from "react";
import { Link } from "react-router";
import type { ModelInfo, VariantInfo } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useConfirm } from "@/components/ui/confirm";
import { Hint } from "@/components/ui/hint";
import { useLoadModel, useUnloadModel } from "@/hooks/use-engine";
import {
	useCancelDownload,
	useDeleteModel,
	useDownloadModel,
	useSetPreferredQuant,
} from "@/hooks/use-models";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { ROUTES } from "@/lib/constants";
import { formatDuration, formatMB } from "@/lib/format";
import type { ModelLatency } from "@/lib/stats";
import { variantKey, variantName } from "@/lib/variant";
import { DownloadProgressBar } from "./download-progress";

interface ModelRowProps {
	model: ModelInfo;
	/** Measured here, over the last week. Absent means "never run" — never
	 *  a catalog claim standing in for a measurement. */
	measured?: ModelLatency;
	isDefault?: boolean;
}

const cell = "num text-[11.5px] text-text-secondary";

const isInstalled = (v: VariantInfo) => v.status === "downloaded";

export function ModelRow({ model, measured, isDefault }: ModelRowProps) {
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

	const installed = model.variants.filter(isInstalled);
	const inFlight = model.variants.filter(
		(v) => v.status === "downloading" || v.status === "queued" || v.updating,
	);
	const failed = model.variants.filter(
		(v) => v.download_error && v.status !== "downloading" && v.status !== "queued" && !v.updating,
	);
	const onDisk = installed.length > 0;
	const preferred = installed.find((v) => v.quant === model.preferred_quant) ?? installed[0];
	// The quants not yet a Variant: what a first download, or "add quant", offers.
	const addable = model.quants.filter((q) => !model.variants.some((v) => v.quant === q.quant));
	const [quant, setQuant] = useState(
		addable.some((q) => q.quant === model.default_quant)
			? model.default_quant
			: (addable[0]?.quant ?? model.default_quant),
	);
	const firstInstallQueued = !onDisk && model.variants.some((v) => v.status === "queued");
	const firstInstallRunning = !onDisk && inFlight.length > 0;
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
						aria-label={loaded ? "resident" : "not loaded"}
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
					{model.retired && <Badge variant="warning">retired</Badge>}
					{model.status === "error" && <Badge variant="error">error</Badge>}
				</div>

				<div className="flex items-center gap-3 pl-[21px] lg:pl-0 lg:contents">
					<span className={`${cell} w-24 hidden lg:block truncate`}>{model.family}</span>
					<span className={`${cell} hidden sm:block lg:hidden truncate`}>{model.family}</span>

					{onDisk ? (
						<span className={`${cell} w-[62px] hidden lg:block`}>
							{installed.length > 1 ? `${installed.length} quants` : (preferred?.quant ?? "gguf")}
						</span>
					) : (
						<QuantSelect model={model} quants={addable} value={quant} onChange={setQuant} />
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

					{/* One fixed slot, so every row's action lines up on the right
					    edge the list aligns on. An installed model's actions are per
					    Variant, on the strip below. */}
					<div className="flex justify-end items-center gap-1.5 shrink-0 ml-auto lg:ml-0 w-[146px] lg:w-[190px]">
						{!onDisk && !firstInstallRunning && (
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
						{firstInstallQueued && (
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
						{onDisk && preferred && (
							<Link
								to={`${ROUTES.EVAL}?model=${encodeURIComponent(variantKey(model.id, preferred.quant))}`}
								aria-label="Evaluation runs for this model"
								className="inline-flex items-center px-1.5 py-1 rounded-md text-text-muted hover:bg-surface-3 hover:text-text-primary transition-colors"
							>
								<FlaskConical size={13} strokeWidth={1.8} />
							</Link>
						)}
					</div>
				</div>
			</div>

			{onDisk && (
				<div className="mt-2 pl-6 flex flex-wrap items-center gap-1.5">
					{installed.map((v) => (
						<VariantChip
							key={v.quant ?? "custom"}
							model={model}
							variant={v}
							preferred={v === preferred}
							choosable={installed.length > 1}
						/>
					))}
					{!model.retired && addable.length > 0 && (
						<AddQuant model={model} quants={addable} value={quant} onChange={setQuant} />
					)}
				</div>
			)}

			{inFlight.map((v) => (
				<div key={v.quant ?? "custom"} className="mt-2 pl-6 flex items-start gap-2">
					{(onDisk || inFlight.length > 1) && (
						<span className="num text-[10.5px] text-text-muted w-14 shrink-0 pt-0.5">
							{v.quant}
						</span>
					)}
					<div className="flex-1 min-w-0">
						<DownloadProgressBar modelId={model.id} quant={v.quant} />
					</div>
				</div>
			))}

			{failed.map((v) => (
				<div
					key={v.quant ?? "custom"}
					className="mt-2 pl-6 flex items-start gap-1.5 text-xs text-error"
				>
					<span className="min-w-0 break-words">
						{variantName(v.quant)} {isInstalled(v) ? "update failed" : "download failed"}:{" "}
						{v.download_error}
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

function QuantSelect({
	model,
	quants,
	value,
	onChange,
}: {
	model: ModelInfo;
	quants: ModelInfo["quants"];
	value: string;
	onChange: (quant: string) => void;
}) {
	return (
		<select
			value={value}
			onChange={(e) => onChange(e.target.value)}
			disabled={quants.length <= 1}
			aria-label={`Quant of ${model.name}`}
			className="num w-[62px] h-6 px-1.5 hidden lg:block bg-surface-3 border border-border rounded-[5px] text-[10.5px] text-text-secondary cursor-pointer"
		>
			{quants.map((q) => (
				<option key={q.quant} value={q.quant}>
					{q.quant}
				</option>
			))}
		</select>
	);
}

/** Download another quant of an installed model: a new Variant beside
 *  the ones already serving. */
function AddQuant({
	model,
	quants,
	value,
	onChange,
}: {
	model: ModelInfo;
	quants: ModelInfo["quants"];
	value: string;
	onChange: (quant: string) => void;
}) {
	const downloadMutation = useDownloadModel();
	const run = useMutationToast(downloadMutation, {
		success: `Downloading ${model.name} ${value}…`,
		error: "Download failed",
	});
	const selected = quants.find((q) => q.quant === value) ?? quants[0];
	return (
		<span className="inline-flex items-center gap-1 h-6 pl-1.5 pr-1 rounded-[5px] border border-dashed border-border text-[10.5px] text-text-muted">
			<select
				value={selected?.quant}
				onChange={(e) => onChange(e.target.value)}
				aria-label={`Quant to add to ${model.name}`}
				className="num bg-transparent text-text-secondary cursor-pointer focus:outline-none"
			>
				{quants.map((q) => (
					<option key={q.quant} value={q.quant}>
						{q.quant} · {formatMB(q.size_mb)}
					</option>
				))}
			</select>
			<button
				type="button"
				aria-label={`Download ${model.name} ${selected?.quant ?? ""}`}
				disabled={downloadMutation.isPending || !selected}
				onClick={() => selected && run({ modelId: model.id, quant: selected.quant })}
				className="inline-flex items-center p-0.5 rounded text-text-muted hover:text-text-primary hover:bg-surface-3 transition-colors cursor-pointer disabled:opacity-60"
			>
				<Plus size={12} strokeWidth={2} />
			</button>
		</span>
	);
}

/** One installed Variant: its quant and size, whether it is loaded, and
 *  what can be done to it alone. */
function VariantChip({
	model,
	variant,
	preferred,
	choosable,
}: {
	model: ModelInfo;
	variant: VariantInfo;
	/** Serves requests that name only the model. */
	preferred: boolean;
	/** More than one Variant is installed, so the choice means something. */
	choosable: boolean;
}) {
	const label = `${model.name} ${variantName(variant.quant)}`;
	const ref = { modelId: model.id, quant: variant.quant };
	const confirm = useConfirm();
	const downloadMutation = useDownloadModel();
	const deleteMutation = useDeleteModel();
	const loadMutation = useLoadModel();
	const unloadMutation = useUnloadModel();
	const preferMutation = useSetPreferredQuant();

	const runUpdate = useMutationToast(downloadMutation, {
		success: `Updating ${label}…`,
		error: "Update failed",
	});
	const runDelete = useMutationToast(deleteMutation, {
		success: `${label} deleted`,
		error: "Delete failed",
	});
	const runLoad = useMutationToast(loadMutation, {
		success: `${label} loaded`,
		error: "Load failed",
	});
	const runUnload = useMutationToast(unloadMutation, {
		success: `${label} unloaded`,
		error: "Unload failed",
	});
	const runPrefer = useMutationToast(preferMutation, {
		success: `${label} now serves requests that name only ${model.name}`,
		error: "Could not change the preferred quant",
	});

	const iconButton =
		"inline-flex items-center p-0.5 rounded text-text-muted hover:bg-surface-3 hover:text-text-primary transition-colors cursor-pointer disabled:opacity-60";

	return (
		<span
			className={`inline-flex items-center gap-1.5 h-6 pl-1.5 pr-1 rounded-[5px] border text-[10.5px] ${
				variant.is_loaded
					? "border-accent-quiet bg-accent-wash text-text-primary"
					: "border-border bg-surface-2 text-text-secondary"
			}`}
		>
			<span
				role="img"
				aria-label={variant.is_loaded ? "resident" : "on disk, not loaded"}
				className={`w-[7px] h-[7px] rounded-full shrink-0 ${
					variant.is_loaded ? "bg-success" : "border border-border"
				}`}
			/>
			<span className="num">{variantName(variant.quant)}</span>
			<span className="num text-text-faint">{formatMB(variant.size_mb)}</span>

			{choosable &&
				variant.quant &&
				(preferred ? (
					<Hint
						label="Preferred"
						content={`Requests that name ${model.name} without a quant use this one.`}
						className="text-accent-ink"
					>
						<Star size={11} strokeWidth={2} fill="currentColor" />
					</Hint>
				) : (
					<button
						type="button"
						aria-label={`Prefer ${label}`}
						disabled={preferMutation.isPending}
						onClick={() => variant.quant && runPrefer({ modelId: model.id, quant: variant.quant })}
						className={iconButton}
					>
						<Star size={11} strokeWidth={1.8} />
					</button>
				))}

			{variant.retired && <Badge variant="warning">retired</Badge>}
			{variant.update_available && !variant.updating && (
				<button
					type="button"
					aria-label={`Update ${label}`}
					disabled={downloadMutation.isPending}
					onClick={async () => {
						const ok = await confirm({
							title: `Update ${label}?`,
							body: `Upstream republished the ${variantName(variant.quant)} file. Updating downloads it again (${formatMB(variant.size_mb)}); the installed file keeps serving until the new one is verified.`,
							confirmLabel: "Update",
						});
						if (ok && variant.quant) runUpdate({ modelId: model.id, quant: variant.quant });
					}}
					className="num inline-flex items-center gap-0.5 px-1 rounded-[3px] bg-accent-wash text-accent-ink hover:brightness-95 cursor-pointer disabled:opacity-60"
				>
					<RefreshCw size={9} strokeWidth={2} />
					update
				</button>
			)}
			{variant.updating && <Badge variant="info">updating</Badge>}

			{variant.is_loaded ? (
				<button
					type="button"
					aria-label={`Unload ${label}`}
					disabled={unloadMutation.isPending}
					onClick={() => runUnload(ref)}
					className={iconButton}
				>
					<Power size={11} strokeWidth={1.8} />
				</button>
			) : (
				<button
					type="button"
					aria-label={`Load ${label}`}
					disabled={loadMutation.isPending}
					onClick={() => runLoad(ref)}
					className={iconButton}
				>
					<Play size={11} strokeWidth={1.8} />
				</button>
			)}

			{!variant.is_loaded && !variant.updating && (
				<button
					type="button"
					aria-label={`Delete ${label}`}
					onClick={async () => {
						const ok = await confirm({
							title: `Delete ${label}?`,
							body: `The ${variantName(variant.quant)} file leaves the disk and has to be downloaded again to come back. Its Home Assistant entity goes with it; the model's other quants keep serving. Evaluation results already recorded for it are kept.`,
							confirmLabel: "Delete",
							destructive: true,
						});
						if (ok) runDelete(ref);
					}}
					className={`${iconButton} hover:bg-error-wash hover:text-error`}
				>
					<Trash2 size={11} strokeWidth={1.8} />
				</button>
			)}
		</span>
	);
}
