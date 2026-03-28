import { EngineControls } from "@/components/engine/engine-controls";
import { PoolStatus } from "@/components/engine/pool-status";
import { TimeoutSettings } from "@/components/engine/timeout-settings";
import { Spinner } from "@/components/ui/spinner";
import { useEngineStatus } from "@/hooks/use-engine";

export function EnginePage() {
	const { data: engine, isLoading } = useEngineStatus();

	if (isLoading) {
		return (
			<div className="flex justify-center py-16">
				<Spinner size="lg" />
			</div>
		);
	}

	return (
		<div className="space-y-6">
			<div>
				<h1 className="text-xl font-bold text-text-primary">Engine</h1>
				<p className="text-sm text-text-secondary mt-1">
					Manage model loading, pool configuration, and engine settings
				</p>
			</div>

			<div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
				<div className="space-y-4">
					<EngineControls defaultModel={engine?.default_model ?? ""} />
					<TimeoutSettings />
				</div>
				<PoolStatus pools={engine?.loaded_pools ?? []} />
			</div>
		</div>
	);
}
