import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { EmptyState } from "@/components/ui/empty-state";
import { useUnloadModel } from "@/hooks/use-engine";
import { Power, PowerOff } from "lucide-react";

interface LoadedModelsProps {
	models: string[];
}

export function LoadedModels({ models }: LoadedModelsProps) {
	const unloadMutation = useUnloadModel();

	if (models.length === 0) {
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
			<CardHeader title="Loaded Models" description={`${models.length} model(s) in memory`} />
			<div className="space-y-4">
				{models.map((modelId) => (
					<div key={modelId} className="p-3 bg-surface-3 rounded-lg">
						<div className="flex items-center justify-between">
							<div className="flex items-center gap-2 min-w-0">
								<span className="text-sm font-medium text-text-primary truncate">
									{modelId}
								</span>
								<Badge variant="success">loaded</Badge>
							</div>
							<Button
								size="sm"
								variant="ghost"
								icon={<PowerOff size={14} />}
								onClick={() => unloadMutation.mutate(modelId)}
								loading={unloadMutation.isPending}
								title="Unload model"
							/>
						</div>
					</div>
				))}
			</div>
		</Card>
	);
}
