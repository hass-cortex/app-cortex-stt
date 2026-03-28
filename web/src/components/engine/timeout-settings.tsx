import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useToast } from "@/components/ui/toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { Save } from "lucide-react";
import { useState } from "react";

export function TimeoutSettings() {
	const { data: settings } = useSettings();
	const updateMutation = useUpdateSettings();
	const { toast } = useToast();

	const [transcriptionTimeout, setTranscriptionTimeout] = useState(
		String(settings?.transcription_timeout_secs ?? 120),
	);
	const [modelLoadTimeout, setModelLoadTimeout] = useState(
		String(settings?.model_load_timeout_secs ?? 300),
	);
	const [poolAcquireTimeout, setPoolAcquireTimeout] = useState(
		String(settings?.pool_acquire_timeout_secs ?? 60),
	);

	const handleSave = () => {
		updateMutation.mutate(
			{
				transcription_timeout_secs: Number.parseInt(transcriptionTimeout, 10),
				model_load_timeout_secs: Number.parseInt(modelLoadTimeout, 10),
				pool_acquire_timeout_secs: Number.parseInt(poolAcquireTimeout, 10),
			},
			{
				onSuccess: () => toast("Timeout settings saved", "success"),
				onError: (err) => toast(`Failed: ${err.message}`, "error"),
			},
		);
	};

	return (
		<Card>
			<CardHeader title="Timeouts" description="Configure timeout limits for engine operations" />
			<div className="space-y-4">
				<Input
					label="Transcription timeout (seconds)"
					type="number"
					min="10"
					max="600"
					value={transcriptionTimeout}
					onChange={(e) => setTranscriptionTimeout(e.target.value)}
				/>
				<Input
					label="Model load timeout (seconds)"
					type="number"
					min="30"
					max="1800"
					value={modelLoadTimeout}
					onChange={(e) => setModelLoadTimeout(e.target.value)}
				/>
				<Input
					label="Pool acquire timeout (seconds)"
					type="number"
					min="5"
					max="300"
					value={poolAcquireTimeout}
					onChange={(e) => setPoolAcquireTimeout(e.target.value)}
				/>
				<Button icon={<Save size={14} />} onClick={handleSave} loading={updateMutation.isPending}>
					Save Timeouts
				</Button>
			</div>
		</Card>
	);
}
