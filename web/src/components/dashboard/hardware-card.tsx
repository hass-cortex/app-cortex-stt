import { Badge } from "@/components/ui/badge";
import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useSystemInfo } from "@/hooks/use-system";
import { formatMB } from "@/lib/format";
import { Cpu, MemoryStick } from "lucide-react";

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

	const ramUsed = data.total_memory_mb - data.available_memory_mb;
	const ramPercent =
		data.total_memory_mb > 0 ? Math.round((ramUsed / data.total_memory_mb) * 100) : 0;

	return (
		<Card>
			<CardHeader title="Hardware" />
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

				<div className="flex flex-wrap gap-1.5 pt-1">
					<Badge variant={data.has_avx ? "success" : "default"}>
						AVX {data.has_avx ? "Yes" : "No"}
					</Badge>
					<Badge variant={data.has_avx2 ? "success" : "default"}>
						AVX2 {data.has_avx2 ? "Yes" : "No"}
					</Badge>
					<Badge variant={data.cuda_available ? "success" : "default"}>
						CUDA {data.cuda_available ? "Yes" : "No"}
					</Badge>
				</div>
			</div>
		</Card>
	);
}
