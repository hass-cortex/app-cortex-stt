import { RefreshCw } from "lucide-react";
import { ModelList } from "@/components/models/model-list";
import { Button } from "@/components/ui/button";
import { useModels, useScanCustomModels } from "@/hooks/use-models";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useStorageInfo } from "@/hooks/use-system";
import { formatBytes } from "@/lib/format";

export function ModelsPage() {
	const { data: models } = useModels();
	const { data: storage } = useStorageInfo();
	const scan = useScanCustomModels();
	const runScan = useMutationToast(scan, {
		success: "Model folder rescanned",
		error: "Rescan failed",
	});

	const installed = (models ?? []).filter(
		(m) => m.status === "downloaded" || m.status === "custom",
	).length;
	const total = models?.length ?? 0;

	return (
		<div className="flex flex-col gap-4">
			<div className="flex items-end justify-between gap-4">
				<div>
					<h1 className="text-[21px] font-semibold tracking-[-0.01em] text-text-primary">Models</h1>
					<p className="text-[12.5px] text-text-secondary mt-1">
						{installed} on disk
						{storage ? ` · ${formatBytes(storage.models_bytes)}` : ""} ·{" "}
						{Math.max(0, total - installed)} more in the catalog
					</p>
				</div>
				<Button
					variant="secondary"
					icon={<RefreshCw size={14} strokeWidth={1.8} />}
					loading={scan.isPending}
					onClick={() => runScan(undefined)}
					aria-label="Rescan folder"
					className="max-sm:w-11 max-sm:h-11 max-sm:px-0"
				>
					<span className="hidden sm:inline">Rescan folder</span>
				</Button>
			</div>
			<ModelList />
		</div>
	);
}
