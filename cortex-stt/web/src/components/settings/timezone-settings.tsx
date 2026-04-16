import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { COMMON_TIMEZONES, getBrowserTimezone } from "@/utils/time";
import { Save } from "lucide-react";
import { useState } from "react";

export function TimezoneSettings() {
	const { data: settings } = useSettings();
	const updateMutation = useUpdateSettings();
	const save = useMutationToast(updateMutation, { success: "Timezone saved" });
	const [timezone, setTimezone] = useState(settings?.timezone ?? "auto");
	const resolvedTz = timezone === "auto" ? getBrowserTimezone() : timezone;

	return (
		<Card>
			<CardHeader title="Timezone" description="Configure how timestamps are displayed" />
			<div className="space-y-3">
				<Select
					options={COMMON_TIMEZONES.map((tz) => ({ value: tz.value, label: tz.label }))}
					value={timezone}
					onChange={(e) => setTimezone(e.target.value)}
				/>
				{timezone === "auto" && <p className="text-xs text-text-muted">Detected: {resolvedTz}</p>}
				<Button
					size="sm"
					icon={<Save size={14} />}
					onClick={() => save({ timezone })}
					loading={updateMutation.isPending}
				>
					Save
				</Button>
			</div>
		</Card>
	);
}
