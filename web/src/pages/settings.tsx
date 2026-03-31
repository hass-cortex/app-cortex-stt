import { CorsSettings } from "@/components/settings/cors-settings";
import { DangerZone } from "@/components/settings/danger-zone";
import { LoggingSettings } from "@/components/settings/logging-settings";
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
					Configure retention, logging, security, and advanced options
				</p>
			</div>

			<div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
				<div className="space-y-4">
					<RetentionSettings />
					<LoggingSettings />
					<TimezoneSettings />
				</div>
				<div className="space-y-4">
					<CorsSettings />
				</div>
			</div>

			<DangerZone />
		</div>
	);
}
