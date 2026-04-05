import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { useToast } from "@/components/ui/toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { Save } from "lucide-react";
import { useState } from "react";

const logLevelOptions = [
	{ value: "error", label: "Error" },
	{ value: "warn", label: "Warning" },
	{ value: "info", label: "Info" },
	{ value: "debug", label: "Debug" },
	{ value: "trace", label: "Trace" },
];

export function LoggingSettings() {
	const { data: settings } = useSettings();
	const updateMutation = useUpdateSettings();
	const { toast } = useToast();
	const [logLevel, setLogLevel] = useState(settings?.log_level ?? "info");

	const handleSave = () => {
		updateMutation.mutate(
			{ log_level: logLevel },
			{
				onSuccess: () => toast("Log level updated", "success"),
				onError: (err) => toast(`Failed: ${err.message}`, "error"),
			},
		);
	};

	return (
		<Card>
			<CardHeader title="Logging" description="Configure log verbosity" />
			<div className="space-y-4">
				<Select
					label="Log level"
					options={logLevelOptions}
					value={logLevel}
					onChange={(e) => setLogLevel(e.target.value)}
				/>
				<Button icon={<Save size={14} />} onClick={handleSave} loading={updateMutation.isPending}>
					Save
				</Button>
			</div>
		</Card>
	);
}
