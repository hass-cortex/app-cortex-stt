import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useToast } from "@/components/ui/toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { Save } from "lucide-react";
import { useState } from "react";

export function RateLimitSettings() {
	const { data: settings } = useSettings();
	const updateMutation = useUpdateSettings();
	const { toast } = useToast();

	const [enabled, setEnabled] = useState(settings?.rate_limit_enabled ?? false);
	const [perMinute, setPerMinute] = useState(String(settings?.rate_limit_per_minute ?? 60));

	const handleSave = () => {
		updateMutation.mutate(
			{
				rate_limit_enabled: enabled,
				rate_limit_per_minute: Number.parseInt(perMinute, 10),
			},
			{
				onSuccess: () => toast("Rate limit settings saved", "success"),
				onError: (err) => toast(`Failed: ${err.message}`, "error"),
			},
		);
	};

	return (
		<Card>
			<CardHeader
				title="Rate Limiting"
				description="Limit the number of API requests per minute per key"
			/>
			<div className="space-y-4">
				<div className="flex items-center gap-3">
					<input
						type="checkbox"
						id="rate-limit-enabled"
						checked={enabled}
						onChange={(e) => setEnabled(e.target.checked)}
						className="rounded border-border accent-accent"
					/>
					<label htmlFor="rate-limit-enabled" className="text-sm text-text-primary cursor-pointer">
						Enable rate limiting
					</label>
				</div>

				{enabled && (
					<Input
						label="Requests per minute"
						type="number"
						min="1"
						max="10000"
						value={perMinute}
						onChange={(e) => setPerMinute(e.target.value)}
					/>
				)}

				<Button icon={<Save size={14} />} onClick={handleSave} loading={updateMutation.isPending}>
					Save
				</Button>
			</div>
		</Card>
	);
}
