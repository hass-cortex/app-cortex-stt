import { Badge } from "@/components/ui/badge";
import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useEngineStatus } from "@/hooks/use-engine";
import { useHealth } from "@/hooks/use-system";
import { Server } from "lucide-react";

export function ModelStatusCard() {
	const { data, isLoading } = useEngineStatus();
	const { data: health } = useHealth();

	if (isLoading) {
		return (
			<Card>
				<CardHeader title="Engine" />
				<div className="flex justify-center py-8">
					<Spinner />
				</div>
			</Card>
		);
	}

	if (!data) return null;

	const loadedModels: string[] = data.loaded_models ?? [];
	const statusVariant =
		health?.status === "ok" ? "success" : health?.status === "degraded" ? "warning" : "info";

	return (
		<Card>
			<CardHeader
				title="Engine"
				action={health && <Badge variant={statusVariant}>{health.status ?? "unknown"}</Badge>}
			/>
			<div className="space-y-3">
				<div className="flex items-center gap-3">
					<Server size={16} className="text-text-muted shrink-0" />
					<div>
						<p className="text-sm text-text-primary">
							{loadedModels.length} model{loadedModels.length !== 1 ? "s" : ""} loaded
						</p>
					</div>
				</div>

				{loadedModels.length > 0 && (
					<div className="space-y-1.5 pt-1">
						{loadedModels.map((modelId) => (
							<div key={modelId} className="flex items-center justify-between text-xs">
								<span className="text-text-secondary truncate mr-2">{modelId}</span>
								<Badge variant="success">loaded</Badge>
							</div>
						))}
					</div>
				)}

				{loadedModels.length === 0 && (
					<p className="text-xs text-text-muted">No models loaded (lazy load on first request)</p>
				)}
			</div>
		</Card>
	);
}
