import { DefaultModel } from "@/components/engine/default-model";
import { ModelLifecycle } from "@/components/engine/model-lifecycle";
import { TranscriptionSettings } from "@/components/engine/timeout-settings";
import { Spinner } from "@/components/ui/spinner";
import { useEngineStatus } from "@/hooks/use-engine";
import { useSettings } from "@/hooks/use-settings";

export function EnginePage() {
	const { data: engine, isLoading: engineLoading } = useEngineStatus();
	const { data: settings, isLoading: settingsLoading } = useSettings();

	if (engineLoading || settingsLoading) {
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
					<DefaultModel defaultModel={settings?.default_model ?? ""} />
					<TranscriptionSettings />
				</div>
				<ModelLifecycle loadedModels={engine?.loaded_models ?? []} />
			</div>
		</div>
	);
}
