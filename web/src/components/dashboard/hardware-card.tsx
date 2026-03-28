import { Badge } from "@/components/ui/badge";
import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useSystemInfo } from "@/hooks/use-system";
import { formatMB } from "@/lib/format";
import { Cpu, MemoryStick, Monitor } from "lucide-react";

export function HardwareCard() {
	const { data, isLoading } = useSystemInfo();

	if (isLoading) {
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

	const ramUsed = data.ram_total_mb - data.ram_available_mb;
	const ramPercent = Math.round((ramUsed / data.ram_total_mb) * 100);

	return (
		<Card>
			<CardHeader title="Hardware" />
			<div className="space-y-3">
				<div className="flex items-center gap-3">
					<Cpu size={16} className="text-text-muted shrink-0" />
					<div className="min-w-0">
						<p className="text-sm text-text-primary truncate">{data.cpu}</p>
						<p className="text-xs text-text-muted">{data.cpu_cores} cores</p>
					</div>
				</div>

				<div className="flex items-center gap-3">
					<MemoryStick size={16} className="text-text-muted shrink-0" />
					<div className="min-w-0">
						<p className="text-sm text-text-primary">
							{formatMB(ramUsed)} / {formatMB(data.ram_total_mb)}
						</p>
						<p className="text-xs text-text-muted">{ramPercent}% used</p>
					</div>
				</div>

				<div className="flex items-center gap-3">
					<Monitor size={16} className="text-text-muted shrink-0" />
					<div className="min-w-0">
						{data.gpu ? (
							<>
								<p className="text-sm text-text-primary truncate">{data.gpu}</p>
								{data.gpu_memory_mb && (
									<p className="text-xs text-text-muted">{formatMB(data.gpu_memory_mb)}</p>
								)}
							</>
						) : (
							<p className="text-sm text-text-muted">No GPU detected</p>
						)}
					</div>
				</div>

				<div className="flex flex-wrap gap-1.5 pt-1">
					<Badge variant={data.has_avx ? "success" : "default"}>
						AVX {data.has_avx ? "Yes" : "No"}
					</Badge>
					<Badge variant={data.has_cuda ? "success" : "default"}>
						CUDA {data.has_cuda ? (data.cuda_version ?? "Yes") : "No"}
					</Badge>
				</div>
			</div>
		</Card>
	);
}
