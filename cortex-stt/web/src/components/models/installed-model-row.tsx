import { FlaskConical, Info, MoreHorizontal, Plus, RefreshCw, X } from "lucide-react";
import { type ReactNode, useRef, useState } from "react";
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
	const evalLabel = installed.length > 1 ? "Compare quants" : "Evaluate";

	return (
		<section className="rounded-[10px] border border-border bg-surface-1">
			<div className="flex items-center gap-x-2.5 min-h-12 pl-4 pr-1 lg:pr-4 py-1.5 bg-surface-3 border-b border-border rounded-t-[10px]">
				{/* Name and facts share a line where there is room, and stack on a phone. */}
				<div className="flex-1 min-w-0 flex flex-col lg:flex-row lg:items-center gap-x-2.5 gap-y-0.5">
					<div className="flex items-center gap-2 min-w-0">
						<h3 className="text-[14px] lg:text-[13px] font-semibold text-text-primary truncate">
							{model.name}
						</h3>
						{model.description && (
							<Hint
								label={model.name}
								content={model.description}
								className="shrink-0 text-text-faint hover:text-text-secondary"
							>
								<Info size={13} strokeWidth={1.8} />
							</Hint>
						)}
						{model.retired && <Badge variant="warning">retired</Badge>}
					</div>
					<span className="num text-[11px] text-text-muted truncate">
						{model.family}
						{model.languages.length > 0 && ` · ${model.languages.length} languages`}
					</span>
				</div>
				<Link
					to={`${ROUTES.EVAL}?model=${encodeURIComponent(evalKeys[0] ?? model.id)}`}
					aria-label={`${evalLabel}: ${model.name}`}
					className="inline-flex items-center justify-center gap-1.5 shrink-0 h-11 w-11 sm:h-8 sm:w-auto sm:px-2.5 rounded-[7px] text-[12px] text-text-secondary hover:bg-surface-2 hover:text-text-primary transition-colors"
				>
					<FlaskConical size={15} strokeWidth={1.8} />
					<span className="hidden sm:inline">{evalLabel}</span>
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
 *  has been here, and what can be done to it alone. A table row from `lg`
 *  up; below, two lines (what it is, then how it is doing) beside its
 *  actions, since the columns' headings are not there to name the figures. */
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

	const dot = (
		<span
			aria-hidden="true"
			className={`w-2 h-2 rounded-full shrink-0 ${
				variant.is_loaded ? "bg-success" : "border-[1.5px] border-border"
			}`}
		/>
	);
	const state = variant.is_loaded ? "Loaded" : "Idle";
	const stateColor = variant.is_loaded ? "text-text-primary" : "text-text-muted";
	const toggle = (
		<Button
			size="sm"
			variant="outline"
			onClick={() => (variant.is_loaded ? runUnload(key) : runLoad(key))}
			loading={loadMutation.isPending || unloadMutation.isPending}
			aria-label={`${variant.is_loaded ? "Unload" : "Load"} ${label}`}
			className="max-lg:h-9 max-lg:px-3.5 max-lg:text-[13px]"
		>
			{variant.is_loaded ? "Unload" : "Load"}
		</Button>
	);
	const p50 = measured ? formatDuration(measured.p50) : "—";
	const p50Color = measured ? "text-text-primary" : "text-text-faint";
	const progress = variant.updating && (
		<DownloadProgressBar modelId={model.id} quant={variant.quant} />
	);

	return (
		<>
			<div className="lg:hidden flex items-center gap-1.5 min-h-16 py-2 pl-4 pr-1">
				<div className="flex-1 min-w-0 flex flex-col gap-1">
					<div className="flex items-baseline gap-2">
						<span className="num text-[14px] font-medium text-text-primary">
							{variantName(variant.quant)}
						</span>
						<span className="num text-[12px] text-text-secondary">{formatMB(variant.size_mb)}</span>
						{variant.retired && <Badge variant="warning">retired</Badge>}
					</div>
					{progress || (
						<div className="flex items-center gap-1.5 text-[12px] text-text-secondary min-w-0">
							{dot}
							<span className={stateColor}>{state}</span>
							<span aria-hidden="true" className="text-text-faint">
								·
							</span>
							<span className="num">
								p50 <span className={p50Color}>{p50}</span>
							</span>
							<span aria-hidden="true" className="text-text-faint">
								·
							</span>
							<span className="num truncate">
								{measured ? `${measured.runs} ${measured.runs === 1 ? "run" : "runs"}` : "no runs"}
							</span>
						</div>
					)}
				</div>
				{!variant.updating && variant.update_available && (
					<UpdateButton model={model} variant={variant} />
				)}
				{!variant.updating && toggle}
				<VariantMenu model={model} variant={variant} />
			</div>

			<div className={`hidden ${VARIANT_GRID} min-h-11 px-4`}>
				<div className="flex items-center gap-2 min-w-0">
					<span className="num text-[12.5px] text-text-primary">{variantName(variant.quant)}</span>
					{variant.retired && <Badge variant="warning">retired</Badge>}
				</div>
				<span className={`${num} text-text-secondary text-right`}>{formatMB(variant.size_mb)}</span>
				<div className="flex items-center gap-2 min-w-0">
					{progress ? (
						<div className="flex-1 min-w-[140px]">{progress}</div>
					) : (
						<>
							{dot}
							<span className={`text-[12px] w-12 ${stateColor}`}>{state}</span>
							{toggle}
							{variant.update_available && <UpdateButton model={model} variant={variant} />}
						</>
					)}
				</div>
				<span className={`${num} text-right ${p50Color}`}>{p50}</span>
				<span className={`${num} text-text-secondary text-right`}>
					{measured ? measured.runs : "—"}
				</span>
				<div className="flex justify-end">
					<VariantMenu model={model} variant={variant} />
				</div>
			</div>
		</>
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
			className="max-lg:h-9"
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

/**
 * What a menu or picker opens into: a popover under its trigger from `sm`
 * up, and below it a sheet along the bottom of the screen over a dimmed
 * page, where a thumb reaches and a popover would run off the edge.
 */
function Overlay({
	open,
	onClose,
	title,
	subtitle,
	width,
	role,
	children,
}: {
	open: boolean;
	onClose: () => void;
	title: ReactNode;
	subtitle?: ReactNode;
	/** The popover's width class from `sm` up. */
	width: string;
	role?: "menu";
	children: ReactNode;
}) {
	if (!open) return null;
	return (
		<>
			<button
				type="button"
				aria-label="Close"
				tabIndex={-1}
				onClick={onClose}
				className="sm:hidden fixed inset-0 z-40 bg-black/60 cursor-default"
			/>
			<div
				role={role}
				className={`fixed inset-x-0 bottom-0 z-50 pt-2 pb-6 bg-surface-2 border-t border-border rounded-t-2xl shadow-lg flex flex-col sm:absolute sm:inset-x-auto sm:bottom-auto sm:right-0 sm:top-[calc(100%+6px)] sm:z-20 ${width} sm:p-1 sm:border sm:rounded-[10px]`}
			>
				<div
					aria-hidden="true"
					className="sm:hidden self-center w-10 h-1 rounded-full bg-border mb-2.5"
				/>
				<div className="sm:hidden px-5 pb-3 flex flex-col gap-0.5">
					<span className="text-[16px] font-semibold text-text-primary">{title}</span>
					{subtitle && <span className="text-[12.5px] text-text-secondary">{subtitle}</span>}
				</div>
				{children}
				<button
					type="button"
					onClick={onClose}
					className="sm:hidden mx-4 mt-3 h-12 rounded-[10px] border border-border bg-surface-3 text-[15px] text-text-primary cursor-pointer"
				>
					Cancel
				</button>
			</div>
		</>
	);
}

/** A row in an Overlay: a full-width tap target on a phone, a compact line on a desktop. */
const overlayItem =
	"w-full text-left flex flex-col justify-center gap-0.5 min-h-14 sm:min-h-9 px-5 sm:px-2.5 border-t border-border-soft sm:border-0 sm:rounded-md text-[15px] sm:text-[12.5px] hover:bg-surface-3 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed";

/** The actions that are rarely wanted, out of the row's way. */
function VariantMenu({ model, variant }: { model: ModelInfo; variant: VariantInfo }) {
	const label = `${model.name} ${variantName(variant.quant)}`;
	const key = variantKey(model.id, variant.quant);
	const [open, setOpen] = useState(false);
	const close = () => setOpen(false);
	const ref = useRef<HTMLDivElement>(null);
	useDismiss(open, close, [ref]);
	const confirm = useConfirm();
	const { toast } = useToast();
	const deleteMutation = useDeleteModel();
	const runDelete = useMutationToast(deleteMutation, {
		success: `${label} uninstalled`,
		error: "Uninstall failed",
	});

	const copyKey = async () => {
		close();
		// Should even the fallback fail, the toast carries the key to copy by hand.
		try {
			await copyToClipboard(key);
			toast(`Copied ${key}`, "success");
		} catch {
			toast(`Variant key: ${key}`, "info");
		}
	};

	const uninstall = async () => {
		close();
		const ok = await confirm({
			title: `Uninstall ${label}?`,
			body: `The ${variantName(variant.quant)} file leaves the disk and has to be downloaded again to come back. Its Home Assistant entity goes with it; the model's other quants keep serving. Evaluation results already recorded for it are kept.`,
			confirmLabel: "Uninstall",
			destructive: true,
		});
		if (ok) runDelete({ modelId: model.id, quant: variant.quant });
	};

	const note = "text-[12px] sm:text-[11px] text-text-muted";

	return (
		<div ref={ref} className="relative">
			<button
				type="button"
				aria-label={`More actions for ${label}`}
				aria-expanded={open}
				onClick={() => setOpen((o) => !o)}
				className="inline-flex items-center justify-center w-11 h-11 lg:w-8 lg:h-8 rounded-[7px] text-text-muted hover:bg-surface-3 hover:text-text-primary transition-colors cursor-pointer"
			>
				<MoreHorizontal size={17} strokeWidth={1.8} />
			</button>
			<Overlay
				open={open}
				onClose={close}
				role="menu"
				width="sm:w-60"
				title={
					<>
						{model.name} <span className="num font-medium">{variantName(variant.quant)}</span>
					</>
				}
				subtitle={`${formatMB(variant.size_mb)} · ${variant.is_loaded ? "Loaded" : "Idle"}`}
			>
				<Link
					to={`${ROUTES.TRANSCRIBE}?model=${encodeURIComponent(key)}`}
					role="menuitem"
					onClick={close}
					className={`${overlayItem} text-text-primary`}
				>
					Try it on Transcribe
					<span className={`sm:hidden ${note}`}>Record a clip with this quant</span>
				</Link>
				<button
					type="button"
					role="menuitem"
					onClick={copyKey}
					className={`${overlayItem} text-text-primary`}
				>
					Copy key
					<span className={`num ${note}`}>{key}</span>
				</button>
				<div className="hidden sm:block h-px bg-border-soft my-1" />
				<button
					type="button"
					role="menuitem"
					onClick={uninstall}
					disabled={variant.is_loaded || variant.updating}
					className={`${overlayItem} text-error`}
				>
					Uninstall {variantName(variant.quant)}
					<span className={note}>
						{variant.is_loaded ? "Unload it first" : "Its Home Assistant entity goes with it"}
					</span>
				</button>
			</Overlay>
		</div>
	);
}

/** Install another quant beside the ones already serving. */
function AddQuant({ model, quants }: { model: ModelInfo; quants: ModelInfo["quants"] }) {
	const [open, setOpen] = useState(false);
	const close = () => setOpen(false);
	const ref = useRef<HTMLDivElement>(null);
	useDismiss(open, close, [ref]);
	const downloadMutation = useDownloadModel();
	const run = useMutationToast(downloadMutation, {
		success: `Downloading another quant of ${model.name}…`,
		error: "Download failed",
	});
	const installed = model.variants.filter(isInstalled).map((v) => variantName(v.quant));

	return (
		<div ref={ref} className="relative shrink-0">
			<Button
				size="sm"
				variant="secondary"
				icon={<Plus size={14} strokeWidth={2} />}
				aria-expanded={open}
				aria-label={`Add a quant of ${model.name}`}
				onClick={() => setOpen((o) => !o)}
				className="max-sm:h-9 max-sm:text-[13px]"
			>
				<span className="sm:hidden">Quant</span>
				<span className="hidden sm:inline">Add quant</span>
			</Button>
			<Overlay
				open={open}
				onClose={close}
				width="sm:w-80"
				title="Add a quant"
				subtitle={`${model.name} · ${installed.join(" and ")} installed`}
			>
				<p className="hidden sm:block px-2.5 pt-1.5 pb-1 text-[12px] text-text-secondary">
					Install another quant of {model.name}
				</p>
				{quants.map((q) => (
					<div
						key={q.quant}
						className="flex items-center gap-3 sm:gap-2 min-h-[60px] sm:min-h-0 py-1 pl-5 pr-4 sm:pl-2.5 sm:pr-1 border-t border-border-soft sm:border-0 rounded-none sm:rounded-md"
					>
						<span className="num flex-1 sm:flex-none sm:w-20 text-[14px] sm:text-[12px] text-text-primary">
							{q.quant}
						</span>
						<span className="num sm:flex-1 text-[12.5px] sm:text-[11.5px] text-text-muted">
							{formatMB(q.size_mb)}
						</span>
						<Button
							size="sm"
							disabled={downloadMutation.isPending}
							className="max-sm:h-10 max-sm:px-4 max-sm:text-[13.5px]"
							onClick={() => {
								close();
								run({ modelId: model.id, quant: q.quant });
							}}
						>
							Download
						</Button>
					</div>
				))}
				<p className="px-5 sm:px-2.5 pt-3 sm:pt-2 pb-1 sm:mt-1 sm:border-t border-border-soft text-[12.5px] sm:text-[11.5px] text-text-muted">
					Installed quants keep serving while it downloads.
				</p>
			</Overlay>
		</div>
	);
}

/** A quant being installed for the first time. */
function InFlightRow({ model, variant }: { model: ModelInfo; variant: VariantInfo }) {
	return (
		<div
			className={`${VARIANT_GRID} flex flex-wrap items-center gap-x-3 gap-y-1 min-h-14 lg:min-h-11 px-4 py-2 lg:py-0`}
		>
			<span className="num text-[13px] lg:text-[12.5px] text-text-secondary">
				{variantName(variant.quant)}
			</span>
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
		<div className="flex flex-wrap items-center gap-2 min-h-11 py-2 lg:py-0 px-4 bg-error-wash last:rounded-b-[10px] text-[12px] text-error">
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
				className="p-2.5 lg:p-1 text-text-muted hover:text-text-primary transition-colors cursor-pointer"
			>
				<X size={13} />
			</button>
		</div>
	);
}
