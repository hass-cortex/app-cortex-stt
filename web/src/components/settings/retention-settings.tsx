import type { RetentionPolicy, RetentionPolicyType } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { useToast } from "@/components/ui/toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { Save } from "lucide-react";
import { useState } from "react";

const policyOptions = [
	{ value: "days", label: "Days" },
	{ value: "count", label: "Record count" },
	{ value: "disk_limit", label: "Disk limit (MB)" },
	{ value: "unlimited", label: "Unlimited" },
];

interface RetentionRowProps {
	label: string;
	policy: RetentionPolicy;
	onChange: (policy: RetentionPolicy) => void;
}

function RetentionRow({ label, policy, onChange }: RetentionRowProps) {
	return (
		<div className="space-y-2">
			<span className="text-sm font-medium text-text-secondary">{label}</span>
			<div className="flex items-center gap-2">
				<Select
					options={policyOptions}
					value={policy.type}
					onChange={(e) =>
						onChange({
							type: e.target.value as RetentionPolicyType,
							value: policy.value,
						})
					}
					className="w-40"
				/>
				{policy.type !== "unlimited" && (
					<Input
						type="number"
						min="1"
						value={String(policy.value ?? "")}
						onChange={(e) =>
							onChange({
								type: policy.type,
								value: Number.parseInt(e.target.value, 10) || undefined,
							})
						}
						className="w-28"
					/>
				)}
			</div>
		</div>
	);
}

export function RetentionSettings() {
	const { data: settings } = useSettings();
	const updateMutation = useUpdateSettings();
	const { toast } = useToast();

	const [saveAudio, setSaveAudio] = useState(settings?.save_audio ?? true);
	const [audioRetention, setAudioRetention] = useState<RetentionPolicy>(
		settings?.audio_retention ?? { type: "days", value: 7 },
	);
	const [recordRetention, setRecordRetention] = useState<RetentionPolicy>(
		settings?.record_retention ?? { type: "days", value: 30 },
	);

	const handleSave = () => {
		updateMutation.mutate(
			{
				save_audio: saveAudio,
				audio_retention: audioRetention,
				record_retention: recordRetention,
			},
			{
				onSuccess: () => toast("Retention settings saved", "success"),
				onError: (err) => toast(`Failed: ${err.message}`, "error"),
			},
		);
	};

	return (
		<Card>
			<CardHeader
				title="Retention Policy"
				description="Configure how long transcription records and audio files are kept"
			/>
			<div className="space-y-4">
				<div className="flex items-center gap-3">
					<input
						type="checkbox"
						id="save-audio"
						checked={saveAudio}
						onChange={(e) => setSaveAudio(e.target.checked)}
						className="rounded border-border accent-accent"
					/>
					<label htmlFor="save-audio" className="text-sm text-text-primary cursor-pointer">
						Save audio files for playback
					</label>
				</div>

				<RetentionRow
					label="Audio file retention"
					policy={audioRetention}
					onChange={setAudioRetention}
				/>
				<RetentionRow
					label="Record retention"
					policy={recordRetention}
					onChange={setRecordRetention}
				/>

				<Button icon={<Save size={14} />} onClick={handleSave} loading={updateMutation.isPending}>
					Save Retention
				</Button>
			</div>
		</Card>
	);
}
