import type { PoolStatus as PoolStatusType } from "@/api/types";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { ProgressBar } from "@/components/ui/progress-bar";
import { useUnloadModel } from "@/hooks/use-engine";
import { formatRelativeTime } from "@/lib/format";
import { Power, PowerOff } from "lucide-react";

interface PoolStatusProps {
	pools: PoolStatusType[];
}

export function PoolStatus({ pools }: PoolStatusProps) {
	const unloadMutation = useUnloadModel();

	if (pools.length === 0) {
		return (
			<Card>
				<CardHeader title="Loaded Models" />
				<EmptyState
					icon={<Power size={32} />}
					title="No models loaded"
					description="Models are loaded on first request or via manual pre-loading."
				/>
			</Card>
		);
	}

	return (
		<Card>
			<CardHeader title="Loaded Models" description={`${pools.length} model(s) in memory`} />
			<div className="space-y-4">
				{pools.map((pool) => {
					const utilization =
						pool.pool_size > 0
							? Math.round(((pool.pool_size - pool.available) / pool.pool_size) * 100)
							: 0;

					return (
						<div key={pool.model_id} className="p-3 bg-surface-3 rounded-lg space-y-2">
							<div className="flex items-center justify-between">
								<div className="flex items-center gap-2 min-w-0">
									<span className="text-sm font-medium text-text-primary truncate">
										{pool.model_id}
									</span>
									<Badge variant={pool.busy > 0 ? "warning" : "success"}>
										{pool.available}/{pool.pool_size} free
									</Badge>
								</div>
								<Button
									size="sm"
									variant="ghost"
									icon={<PowerOff size={14} />}
									onClick={() => unloadMutation.mutate(pool.model_id)}
									loading={unloadMutation.isPending}
									title="Unload model"
								/>
							</div>

							<ProgressBar
								value={utilization}
								variant={utilization > 80 ? "warning" : "accent"}
								size="sm"
							/>

							<div className="flex items-center justify-between text-xs text-text-muted">
								<span>
									{pool.busy} busy, {pool.available} available
								</span>
								{pool.last_used && <span>Last used: {formatRelativeTime(pool.last_used)}</span>}
							</div>
						</div>
					);
				})}
			</div>
		</Card>
	);
}
