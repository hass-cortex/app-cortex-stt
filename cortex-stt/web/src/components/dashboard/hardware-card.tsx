import { Cpu, Database, HardDrive, MemoryStick, Mic, Monitor, Package } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Card, CardHeader } from "@/components/ui/card";
import { ProgressBar } from "@/components/ui/progress-bar";
import { Spinner } from "@/components/ui/spinner";
import { useStorageInfo, useSystemInfo } from "@/hooks/use-system";
import { formatBytes, formatMB } from "@/lib/format";

export function HardwareCard() {
	const { data, isLoading } = useSystemInfo();
	const { data: storage, isLoading: storageLoading } = useStorageInfo();

	if (isLoading || storageLoading) {
		return (
			<Card>
				<CardHeader title="Hardware" />
				<div className="flex justify-center py-8">
					<Spinner />
				</div>
			</Card>
		);
	}

	if (!data) return null;

	const ramUsed = data.total_memory_mb - data.available_memory_mb;
	const ramPercent =
		data.total_memory_mb > 0 ? Math.round((ramUsed / data.total_memory_mb) * 100) : 0;

	const storageUsed = storage
		? storage.models_bytes + storage.audio_bytes + storage.database_bytes
		: 0;
	const storageTotal = storage ? storageUsed + storage.free_bytes : 0;
	const storagePercent = storageTotal > 0 ? Math.round((storageUsed / storageTotal) * 100) : 0;

	const storageItems = storage
		? [
				{ icon: Package, label: "Models", bytes: storage.models_bytes },
				{ icon: Mic, label: "Audio", bytes: storage.audio_bytes },
				{ icon: Database, label: "Database", bytes: storage.database_bytes },
			]
		: [];

	return (
		<Card>
			<CardHeader title="Hardware" />
			<div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
				{/* System */}
				<div className="space-y-3">
					<div className="flex items-center gap-3">
						<Cpu size={16} className="text-text-muted shrink-0" />
						<div className="min-w-0">
							<p className="text-sm text-text-primary">{data.cpu_count} CPU cores</p>
							<p className="text-xs text-text-muted">
								{data.os} / {data.arch}
							</p>
						</div>
					</div>

					<div className="flex items-center gap-3">
						<MemoryStick size={16} className="text-text-muted shrink-0" />
						<div className="min-w-0">
							<p className="text-sm text-text-primary">
								{formatMB(ramUsed)} / {formatMB(data.total_memory_mb)}
							</p>
							<p className="text-xs text-text-muted">{ramPercent}% used</p>
						</div>
					</div>

					{data.gpu_info ? (
						<div className="flex items-center gap-3">
							<Monitor size={16} className="text-text-muted shrink-0" />
							<div className="min-w-0">
								<p className="text-sm text-text-primary">{data.gpu_info.name}</p>
								<p className="text-xs text-text-muted">
									{formatMB(data.gpu_info.memory_used_mb)} /{" "}
									{formatMB(data.gpu_info.memory_total_mb)} VRAM
								</p>
							</div>
						</div>
					) : null}

					<div className="flex flex-wrap gap-1.5">
						<Badge variant={data.has_avx ? "success" : "default"}>
							AVX {data.has_avx ? "✓" : "✗"}
						</Badge>
						<Badge variant={data.has_avx2 ? "success" : "default"}>
							AVX2 {data.has_avx2 ? "✓" : "✗"}
						</Badge>
						<Badge variant={data.cuda_available ? "success" : "default"}>
							CUDA {data.cuda_available ? "✓" : "✗"}
						</Badge>
					</div>
				</div>

				{/* Storage */}
				{storage && (
					<div className="space-y-3">
						<div className="flex items-center gap-3">
							<HardDrive size={16} className="text-text-muted shrink-0" />
							<div className="flex-1">
								<div className="flex items-center justify-between text-sm mb-1">
									<span className="text-text-primary">{formatBytes(storageUsed)} used</span>
									<span className="text-text-muted">{formatBytes(storage.free_bytes)} free</span>
								</div>
								<ProgressBar
									value={storagePercent}
									variant={
										storagePercent > 90 ? "error" : storagePercent > 70 ? "warning" : "success"
									}
									size="sm"
								/>
							</div>
						</div>

						<div className="space-y-2">
							{storageItems.map(({ icon: Icon, label, bytes }) => (
								<div key={label} className="flex items-center justify-between text-xs">
									<div className="flex items-center gap-2 text-text-secondary">
										<Icon size={12} className="text-text-muted" />
										{label}
									</div>
									<span className="text-text-primary">{formatBytes(bytes)}</span>
								</div>
							))}
						</div>
					</div>
				)}
			</div>
		</Card>
	);
}
