import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useToast } from "@/components/ui/toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { Save } from "lucide-react";
import { useState } from "react";

export function TranscriptionSettings() {
	const { data: settings } = useSettings();
	const updateMutation = useUpdateSettings();
	const { toast } = useToast();

	const [transcriptionTimeout, setTranscriptionTimeout] = useState(
		String(settings?.transcription_timeout_secs ?? 120),
	);

	const handleSave = () => {
		updateMutation.mutate(
			{ transcription_timeout_secs: Number.parseInt(transcriptionTimeout, 10) },
			{
				onSuccess: () => toast("Transcription settings saved", "success"),
				onError: (err) => toast(`Failed: ${err.message}`, "error"),
			},
		);
	};

	return (
		<Card>
			<CardHeader title="Transcription" description="Configure transcription operation limits" />
			<div className="space-y-4">
				<Input
					label="Timeout (seconds)"
					type="number"
					min="10"
					max="600"
					value={transcriptionTimeout}
					onChange={(e) => setTranscriptionTimeout(e.target.value)}
				/>
				<p className="text-xs text-text-muted">
					Maximum time allowed for a single transcription request before it is cancelled.
				</p>
				<Button icon={<Save size={14} />} onClick={handleSave} loading={updateMutation.isPending}>
					Save
				</Button>
			</div>
		</Card>
	);
}
