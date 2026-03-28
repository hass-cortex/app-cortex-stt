import { Badge } from "@/components/ui/badge";
import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useEngineStatus } from "@/hooks/use-engine";
import { useHealth } from "@/hooks/use-system";
import { Layers } from "lucide-react";

export function QueueCard() {
	const { data: engine, isLoading: engineLoading } = useEngineStatus();
	const { data: health, isLoading: healthLoading } = useHealth();

	if (engineLoading || healthLoading) {
		return (
			<Card>
				<CardHeader title="Status" />
				<div className="flex justify-center py-8">
					<Spinner />
				</div>
			</Card>
		);
	}

	const statusVariant =
		health?.status === "ok" ? "success" : health?.status === "degraded" ? "warning" : "info";

	return (
		<Card>
			<CardHeader
				title="Status"
				action={health && <Badge variant={statusVariant}>{health.status}</Badge>}
			/>
			<div className="space-y-3">
				<div className="flex items-center gap-3">
					<Layers size={16} className="text-text-muted shrink-0" />
					<div>
						<p className="text-sm text-text-primary">Queue depth: {engine?.queue_depth ?? 0}</p>
						<p className="text-xs text-text-muted">
							{engine?.queue_depth === 0 ? "No pending requests" : "Requests waiting for pool"}
						</p>
					</div>
				</div>

				{health && (
					<div className="pt-2 border-t border-border">
						<p className="text-xs text-text-muted">
							Version: <span className="text-text-secondary">{health.version}</span>
						</p>
					</div>
				)}
			</div>
		</Card>
	);
}
