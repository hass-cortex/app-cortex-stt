import { Badge } from "@/components/ui/badge";
import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useEngineStatus } from "@/hooks/use-engine";
import { Activity, Server } from "lucide-react";

export function ModelStatusCard() {
	const { data, isLoading } = useEngineStatus();

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

	return (
		<Card>
			<CardHeader title="Engine" />
			<div className="space-y-3">
				<div className="flex items-center gap-3">
					<Activity size={16} className="text-accent shrink-0" />
					<div>
						<p className="text-sm font-medium text-text-primary">{data.default_model}</p>
						<p className="text-xs text-text-muted">Default model</p>
					</div>
				</div>

				<div className="flex items-center gap-3">
					<Server size={16} className="text-text-muted shrink-0" />
					<div>
						<p className="text-sm text-text-primary">
							{data.loaded_pools.length} / {data.max_loaded_models} models loaded
						</p>
					</div>
				</div>

				{data.loaded_pools.length > 0 && (
					<div className="space-y-1.5 pt-1">
						{data.loaded_pools.map((pool) => (
							<div key={pool.model_id} className="flex items-center justify-between text-xs">
								<span className="text-text-secondary truncate mr-2">{pool.model_id}</span>
								<div className="flex items-center gap-1.5 shrink-0">
									<Badge variant="success">{pool.available} free</Badge>
									{pool.busy > 0 && <Badge variant="warning">{pool.busy} busy</Badge>}
								</div>
							</div>
						))}
					</div>
				)}
			</div>
		</Card>
	);
}
