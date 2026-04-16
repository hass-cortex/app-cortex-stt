import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { Save } from "lucide-react";
import { useState } from "react";

export function EngineSettings() {
	const { data: settings } = useSettings();
	const updateMutation = useUpdateSettings();
	const save = useMutationToast(updateMutation, { success: "Engine settings saved" });

	const currentIdle = settings?.idle_timeout_secs;
	const [keepLoaded, setKeepLoaded] = useState(currentIdle === null || currentIdle === undefined);
	const [idleTimeout, setIdleTimeout] = useState(String(currentIdle ?? 300));
	const [maxLoaded, setMaxLoaded] = useState(String(settings?.max_loaded_models ?? 1));
	const [transcriptionTimeout, setTranscriptionTimeout] = useState(
		String(settings?.transcription_timeout_secs ?? 120),
	);

	const handleSave = () => {
		save({
			idle_timeout_secs: keepLoaded ? null : Number.parseInt(idleTimeout, 10),
			max_loaded_models: Number.parseInt(maxLoaded, 10) || 1,
			transcription_timeout_secs: Number.parseInt(transcriptionTimeout, 10),
		});
	};

	return (
		<Card>
			<CardHeader title="Engine" description="Model lifecycle and transcription limits" />
			<div className="space-y-4">
				<label className="flex items-center gap-2 cursor-pointer">
					<input
						type="checkbox"
						checked={keepLoaded}
						onChange={(e) => setKeepLoaded(e.target.checked)}
						className="rounded border-border"
					/>
					<span className="text-sm text-text-primary">Keep models loaded (no auto-unload)</span>
				</label>
				{!keepLoaded && (
					<Input
						label="Idle timeout (seconds)"
						type="number"
						min="30"
						max="3600"
						value={idleTimeout}
						onChange={(e) => setIdleTimeout(e.target.value)}
					/>
				)}
				<p className="text-xs text-text-muted">
					{keepLoaded
						? "Models stay in memory until manually unloaded or server restart."
						: "Models are unloaded after being idle for this duration."}
				</p>

				<Input
					label="Max loaded models"
					type="number"
					min="1"
					max="10"
					value={maxLoaded}
					onChange={(e) => setMaxLoaded(e.target.value)}
				/>
				<p className="text-xs text-text-muted">
					Maximum models in memory simultaneously. Oldest model is unloaded when limit is reached.
				</p>

				<hr className="border-border" />

				<Input
					label="Transcription timeout (seconds)"
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
