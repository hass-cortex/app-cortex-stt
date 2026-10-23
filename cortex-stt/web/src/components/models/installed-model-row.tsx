import { FlaskConical, Info, MoreHorizontal, Plus, RefreshCw, X } from "lucide-react";
import { useRef, useState } from "react";
import { Link } from "react-router";
import type { ModelInfo, VariantInfo } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { useConfirm } from "@/components/ui/confirm";
import { Hint } from "@/components/ui/hint";
import { useToast } from "@/components/ui/toast";
import { useDismiss } from "@/hooks/use-dismiss";
import { useLoadModel, useUnloadModel } from "@/hooks/use-engine";
import { useCancelDownload, useDeleteModel, useDownloadModel } from "@/hooks/use-models";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { copyToClipboard } from "@/lib/clipboard";
import { ROUTES } from "@/lib/constants";
import { formatDuration, formatMB } from "@/lib/format";
import type { ModelLatency } from "@/lib/stats";
import { variantKey, variantName } from "@/lib/variant";
import { DownloadProgressBar } from "./download-progress";

/** Columns shared by the head and every Variant row, so they line up. */
export const VARIANT_GRID =
	"lg:grid lg:grid-cols-[minmax(0,1fr)_88px_196px_104px_60px_40px] lg:items-center lg:gap-x-3";

const num = "num text-[11.5px]";

const isInstalled = (v: VariantInfo) => v.status === "downloaded";

const isInFlight = (v: VariantInfo) =>
	v.status === "downloading" || v.status === "queued" || v.updating;

export function InstalledModelHead() {
	const head = "num text-[10px] tracking-[0.06em] text-text-faint";
	return (
		<div className={`hidden ${VARIANT_GRID} px-[17px] pb-1.5`}>
			<span className={head}>MODEL · QUANT</span>
			<span className={`${head} text-right`}>SIZE</span>
			<span className={head}>MEMORY</span>
			<span className={`${head} text-right`}>MEASURED p50</span>
			<span className={`${head} text-right`}>RUNS</span>
			<span />
		</div>
	);
}

interface InstalledModelRowProps {
	model: ModelInfo;
	/** Measured latency by Variant key, over the last week. */
	measured: Map<string, ModelLatency>;
}

/** A model with Variants on disk: the model as a heading, each installed
 *  quant as its own row, and the quants still to install behind one button. */
export function InstalledModelRow({ model, measured }: InstalledModelRowProps) {
	const installed = model.variants.filter(isInstalled);
	const firstInstalls = model.variants.filter((v) => !isInstalled(v) && isInFlight(v));
	const failed = model.variants.filter((v) => v.download_error && !isInFlight(v));
	const addable = model.quants.filter((q) => !model.variants.some((v) => v.quant === q.quant));
	const evalKeys = installed.map((v) => variantKey(model.id, v.quant));

	return (
		<section className="rounded-[10px] border border-border bg-surface-1">
			<div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 min-h-12 px-4 py-1.5 bg-surface-3 border-b border-border rounded-t-[10px]">
				<h3 className="text-[13px] font-semibold text-text-primary">{model.name}</h3>
				{model.description && (
					<Hint
						label={model.name}
						content={model.description}
						className="shrink-0 text-text-faint hover:text-text-secondary"
					>
						<Info size={13} strokeWidth={1.8} />
					</Hint>
				)}
				<span className="num text-[11px] text-text-muted">
					{model.family}
					{model.languages.length > 0 && ` · ${model.languages.length} languages`}
				</span>
				{model.retired && <Badge variant="warning">retired</Badge>}
				<span className="flex-1" />
				<Link
					to={`${ROUTES.EVAL}?model=${encodeURIComponent(evalKeys[0] ?? model.id)}`}
					className="inline-flex items-center gap-1.5 h-8 px-2.5 rounded-[7px] text-[12px] text-text-secondary hover:bg-surface-3 hover:text-text-primary transition-colors"
				>
					<FlaskConical size={14} strokeWidth={1.8} />
					{installed.length > 1 ? "Compare quants" : "Evaluate"}
				</Link>
				{!model.retired && addable.length > 0 && <AddQuant model={model} quants={addable} />}
			</div>

			<div className="divide-y divide-border-soft">
				{installed.map((v) => (
					<VariantRow
						key={v.quant ?? "custom"}
						model={model}
						variant={v}
						measured={measured.get(variantKey(model.id, v.quant))}
					/>
				))}
				{firstInstalls.map((v) => (
					<InFlightRow key={v.quant ?? "custom"} model={model} variant={v} />
				))}
				{failed.map((v) => (
					<FailedRow key={v.quant ?? "custom"} model={model} variant={v} />
				))}
			</div>
		</section>
	);
}

/** One installed Variant: what it is, whether it is resident, how fast it
 *  has been here, and what can be done to it alone. */
function VariantRow({
	model,
	variant,
	measured,
}: {
	model: ModelInfo;
	variant: VariantInfo;
	measured?: ModelLatency;
}) {
	const label = `${model.name} ${variantName(variant.quant)}`;
	const key = variantKey(model.id, variant.quant);
	const loadMutation = useLoadModel();
	const unloadMutation = useUnloadModel();
	const runLoad = useMutationToast(loadMutation, {
		success: `${label} loaded`,
		error: "Load failed",
	});
	const runUnload = useMutationToast(unloadMutation, {
		success: `${label} unloaded`,
		error: "Unload failed",
	});

	return (
		<div
			className={`${VARIANT_GRID} flex flex-wrap items-center gap-x-3 gap-y-1 min-h-11 px-4 py-1.5 lg:py-0`}
		>
			<div className="flex items-center gap-2 min-w-0 basis-full lg:basis-auto">
				<span className="num text-[12.5px] text-text-primary">{variantName(variant.quant)}</span>
				{variant.retired && <Badge variant="warning">retired</Badge>}
			</div>
			<span className={`${num} text-text-secondary lg:text-right`}>
				{formatMB(variant.size_mb)}
			</span>

			<div className="flex items-center gap-2 min-w-0">
				{variant.updating ? (
					<div className="flex-1 min-w-[140px]">
						<DownloadProgressBar modelId={model.id} quant={variant.quant} />
					</div>
				) : (
					<>
						<span
							aria-hidden="true"
							className={`w-2 h-2 rounded-full shrink-0 ${
								variant.is_loaded ? "bg-success" : "border-[1.5px] border-border"
							}`}
						/>
						<span
							className={`text-[12px] w-12 ${variant.is_loaded ? "text-text-primary" : "text-text-muted"}`}
						>
							{variant.is_loaded ? "Loaded" : "Idle"}
						</span>
						<Button
							size="sm"
							variant="outline"
							onClick={() => (variant.is_loaded ? runUnload(key) : runLoad(key))}
							loading={loadMutation.isPending || unloadMutation.isPending}
							aria-label={`${variant.is_loaded ? "Unload" : "Load"} ${label}`}
						>
							{variant.is_loaded ? "Unload" : "Load"}
						</Button>
						{variant.update_available && <UpdateButton model={model} variant={variant} />}
					</>
				)}
			</div>

			<span
				className={`${num} lg:text-right ${measured ? "text-text-primary" : "text-text-faint"}`}
			>
				<span className="lg:hidden text-text-faint">p50 </span>
				{measured ? formatDuration(measured.p50) : "—"}
			</span>
			<span className={`${num} text-text-secondary text-right hidden lg:block`}>
				{measured ? measured.runs : "—"}
			</span>
			<div className="ml-auto lg:ml-0 flex justify-end">
				<VariantMenu model={model} variant={variant} />
			</div>
		</div>
	);
}

function UpdateButton({ model, variant }: { model: ModelInfo; variant: VariantInfo }) {
	const label = `${model.name} ${variantName(variant.quant)}`;
	const confirm = useConfirm();
	const downloadMutation = useDownloadModel();
	const runUpdate = useMutationToast(downloadMutation, {
		success: `Updating ${label}…`,
		error: "Update failed",
	});
	return (
		<Button
			size="sm"
			variant="secondary"
			icon={<RefreshCw size={12} strokeWidth={2} />}
			loading={downloadMutation.isPending}
			onClick={async () => {
				const ok = await confirm({
					title: `Update ${label}?`,
					body: `Upstream republished the ${variantName(variant.quant)} file. Updating downloads it again (${formatMB(variant.size_mb)}); the installed file keeps serving until the new one is verified.`,
					confirmLabel: "Update",
				});
				if (ok && variant.quant) runUpdate({ modelId: model.id, quant: variant.quant });
			}}
		>
			Update
		</Button>
	);
}

/** The actions that are rarely wanted, out of the row's way. */
function VariantMenu({ model, variant }: { model: ModelInfo; variant: VariantInfo }) {
	const label = `${model.name} ${variantName(variant.quant)}`;
	const key = variantKey(model.id, variant.quant);
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLDivElement>(null);
	useDismiss(open, () => setOpen(false), [ref]);
	const confirm = useConfirm();
	const { toast } = useToast();
	const deleteMutation = useDeleteModel();
	const runDelete = useMutationToast(deleteMutation, {
		success: `${label} uninstalled`,
		error: "Uninstall failed",
	});

	const copyKey = async () => {
		setOpen(false);
		// Should even the fallback fail, the toast carries the key to copy by hand.
		try {
			await copyToClipboard(key);
			toast(`Copied ${key}`, "success");
		} catch {
			toast(`Variant key: ${key}`, "info");
		}
	};

	const uninstall = async () => {
		setOpen(false);
		const ok = await confirm({
			title: `Uninstall ${label}?`,
			body: `The ${variantName(variant.quant)} file leaves the disk and has to be downloaded again to come back. Its Home Assistant entity goes with it; the model's other quants keep serving. Evaluation results already recorded for it are kept.`,
			confirmLabel: "Uninstall",
			destructive: true,
		});
		if (ok) runDelete({ modelId: model.id, quant: variant.quant });
	};

	const item =
		"w-full text-left h-9 px-2.5 rounded-md text-[12.5px] hover:bg-surface-3 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed";

	return (
		<div ref={ref} className="relative">
			<button
				type="button"
				aria-label={`More actions for ${label}`}
				aria-expanded={open}
				onClick={() => setOpen((o) => !o)}
				className="inline-flex items-center justify-center w-8 h-8 rounded-[7px] text-text-muted hover:bg-surface-3 hover:text-text-primary transition-colors cursor-pointer"
			>
				<MoreHorizontal size={16} strokeWidth={1.8} />
			</button>
			{open && (
				<div
					role="menu"
					className="absolute right-0 top-9 z-20 w-52 p-1 bg-surface-2 border border-border rounded-[10px] shadow-lg flex flex-col"
				>
					<button
						type="button"
						role="menuitem"
						onClick={copyKey}
						className={`${item} text-text-primary`}
					>
						Copy key <span className="num text-text-muted">{key}</span>
					</button>
					<div className="h-px bg-border-soft my-1" />
					<button
						type="button"
						role="menuitem"
						onClick={uninstall}
						disabled={variant.is_loaded || variant.updating}
						title={variant.is_loaded ? "Unload it first" : undefined}
						className={`${item} text-error`}
					>
						Uninstall {variantName(variant.quant)}
					</button>
				</div>
			)}
		</div>
	);
}

/** Install another quant beside the ones already serving. */
function AddQuant({ model, quants }: { model: ModelInfo; quants: ModelInfo["quants"] }) {
	const [open, setOpen] = useState(false);
	const ref = useRef<HTMLDivElement>(null);
	useDismiss(open, () => setOpen(false), [ref]);
	const downloadMutation = useDownloadModel();
	const run = useMutationToast(downloadMutation, {
		success: `Downloading another quant of ${model.name}…`,
		error: "Download failed",
	});

	return (
		<div ref={ref} className="relative">
			<Button
				size="sm"
				variant="secondary"
				icon={<Plus size={14} strokeWidth={2} />}
				aria-expanded={open}
				onClick={() => setOpen((o) => !o)}
			>
				Add quant
			</Button>
			{open && (
				<div className="absolute right-0 top-10 z-20 w-80 p-1.5 bg-surface-2 border border-border rounded-[10px] shadow-lg">
					<p className="px-2.5 pt-1.5 pb-1 text-[12px] text-text-secondary">
						Install another quant of {model.name}
					</p>
					{quants.map((q) => (
						<div
							key={q.quant}
							className="grid grid-cols-[80px_minmax(0,1fr)_auto] items-center gap-2 py-1 pl-2.5 pr-1 rounded-md"
						>
							<span className="num text-[12px] text-text-primary">{q.quant}</span>
							<span className="num text-[11.5px] text-text-muted">{formatMB(q.size_mb)}</span>
							<Button
								size="sm"
								disabled={downloadMutation.isPending}
								onClick={() => {
									setOpen(false);
									run({ modelId: model.id, quant: q.quant });
								}}
							>
								Download
							</Button>
						</div>
					))}
					<p className="px-2.5 pt-2 pb-1 mt-1 border-t border-border-soft text-[11.5px] text-text-muted">
						Installed quants keep serving while it downloads.
					</p>
				</div>
			)}
		</div>
	);
}

/** A quant being installed for the first time. */
function InFlightRow({ model, variant }: { model: ModelInfo; variant: VariantInfo }) {
	return (
		<div className={`${VARIANT_GRID} flex flex-wrap items-center gap-x-3 gap-y-1 min-h-11 px-4`}>
			<span className="num text-[12.5px] text-text-secondary">{variantName(variant.quant)}</span>
			<span className={`${num} text-text-muted lg:text-right`}>{formatMB(variant.size_mb)}</span>
			<div className="basis-full lg:basis-auto lg:col-span-4">
				<DownloadProgressBar modelId={model.id} quant={variant.quant} />
			</div>
		</div>
	);
}

/** A download that failed, kept until a retry or a dismiss. */
function FailedRow({ model, variant }: { model: ModelInfo; variant: VariantInfo }) {
	const downloadMutation = useDownloadModel();
	const cancelMutation = useCancelDownload();
	return (
		<div className="flex flex-wrap items-center gap-2 min-h-11 px-4 bg-error-wash last:rounded-b-[10px] text-[12px] text-error">
			<span className="min-w-0 flex-1 break-words">
				<span className="num">{variantName(variant.quant)}</span>{" "}
				{isInstalled(variant) ? "update failed" : "download failed"}: {variant.download_error}
			</span>
			{variant.quant && (
				<Button
					size="sm"
					variant="secondary"
					loading={downloadMutation.isPending}
					onClick={() =>
						variant.quant && downloadMutation.mutate({ modelId: model.id, quant: variant.quant })
					}
				>
					Retry
				</Button>
			)}
			<button
				type="button"
				aria-label="Dismiss download error"
				onClick={() => cancelMutation.mutate({ modelId: model.id, quant: variant.quant })}
				className="p-1 text-text-muted hover:text-text-primary transition-colors cursor-pointer"
			>
				<X size={13} />
			</button>
		</div>
	);
}
