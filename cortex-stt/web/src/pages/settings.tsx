import { DangerZone } from "@/components/settings/danger-zone";
import { DefaultModelSettings } from "@/components/settings/default-model-settings";
import { EngineSettings } from "@/components/settings/engine-settings";
import { RetentionSettings } from "@/components/settings/retention-settings";
import { TimezoneSettings } from "@/components/settings/timezone-settings";
import { Spinner } from "@/components/ui/spinner";
import { useSettings } from "@/hooks/use-settings";

export function SettingsPage() {
	const { isLoading } = useSettings();

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
				<h1 className="text-xl font-bold text-text-primary">Settings</h1>
				<p className="text-sm text-text-secondary mt-1">
					Configure models, engine behavior, retention, and advanced options
				</p>
			</div>

			<div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
				<DefaultModelSettings />
				<EngineSettings />
				<RetentionSettings />
				<TimezoneSettings />
			</div>

			<DangerZone />
		</div>
	);
}
