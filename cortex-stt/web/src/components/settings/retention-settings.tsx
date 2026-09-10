import { AudioLines, Clock, Save } from "lucide-react";
import { useState } from "react";
import type { RetentionPolicy, RetentionPolicyType } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Segmented } from "@/components/ui/segmented";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { useMetrics, useStorageInfo } from "@/hooks/use-system";
import { formatBytes, formatNumber } from "@/lib/format";

const policyOptions: { value: RetentionPolicyType; label: string }[] = [
	{ value: "Days", label: "Days" },
	{ value: "Count", label: "Count" },
	{ value: "DiskLimitMb", label: "Disk" },
	{ value: "Unlimited", label: "Keep all" },
];

const unitFor: Record<RetentionPolicyType, string> = {
	Days: "days",
	Count: "records",
	DiskLimitMb: "MB",
	Unlimited: "",
};

interface PolicyPanelProps {
	title: string;
	icon: typeof Clock;
	explanation: string;
	current: string;
	policy: RetentionPolicy;
	onChange: (policy: RetentionPolicy) => void;
}

function PolicyPanel({
	title,
	icon: Icon,
	explanation,
	current,
	policy,
	onChange,
}: PolicyPanelProps) {
	return (
		<div className="flex-1 min-w-0 px-4 py-3.5 bg-surface-3 rounded-lg">
			<div className="flex items-center gap-2">
				<Icon size={15} strokeWidth={1.7} className="text-text-secondary" />
				<span className="text-[12.5px] font-semibold text-text-primary">{title}</span>
			</div>
			<p className="mt-1.5 mb-3 text-[11.5px] leading-relaxed text-text-muted">{explanation}</p>

			<Segmented
				options={policyOptions}
				value={policy.type}
				onChange={(type) => onChange({ type, value: policy.value })}
			/>

			<div className="mt-3 flex items-center gap-2.5">
				{policy.type !== "Unlimited" && (
					<div className="num flex items-center justify-between w-[132px] h-[34px] px-[11px] bg-surface-2 border border-border rounded-[7px]">
						<input
							type="number"
							min="1"
							value={String(policy.value ?? "")}
							onChange={(e) =>
								onChange({
									type: policy.type,
									value: Number.parseInt(e.target.value, 10) || undefined,
								})
							}
							className="w-full bg-transparent text-[12.5px] text-text-primary focus:outline-none"
						/>
						<span className="text-text-muted text-[12px] shrink-0">{unitFor[policy.type]}</span>
					</div>
				)}
				<span className="num text-[11px] text-text-faint">{current}</span>
			</div>
		</div>
	);
}

/**
 * Two independent policies, side by side. They are allowed to disagree —
 * keeping rows for a month while keeping their audio only while it fits
 * under a cap is the normal case, not a misconfiguration.
 */
export function RetentionSettings() {
	const { data: settings } = useSettings();
	const { data: metrics } = useMetrics();
	const { data: storage } = useStorageInfo();
	const updateMutation = useUpdateSettings();
	const save = useMutationToast(updateMutation, { success: "Retention settings saved" });

	const [saveAudio, setSaveAudio] = useState(settings?.save_audio ?? true);
	const [audioRetention, setAudioRetention] = useState<RetentionPolicy>(
		settings?.audio_retention ?? { type: "Days", value: 7 },
	);
	const [recordRetention, setRecordRetention] = useState<RetentionPolicy>(
		settings?.record_retention ?? { type: "Days", value: 30 },
	);

	return (
		<Card>
			<CardHeader
				title="Retention"
				description="two independent policies · the sweep runs hourly"
				action={
					<Button
						size="sm"
						icon={<Save size={13} strokeWidth={1.8} />}
						loading={updateMutation.isPending}
						onClick={() =>
							save({
								save_audio: saveAudio,
								audio_retention: audioRetention,
								record_retention: recordRetention,
							})
						}
					>
						Save
					</Button>
				}
			/>

			<div className="mt-4 flex flex-col lg:flex-row gap-4">
				<PolicyPanel
					title="Records"
					icon={Clock}
					explanation="Drops the row and its audio together."
					current={`${formatNumber(metrics?.total_transcriptions ?? 0)} rows now`}
					policy={recordRetention}
					onChange={setRecordRetention}
				/>
				<PolicyPanel
					title="Audio"
					icon={AudioLines}
					explanation="Removes the file, keeps the row and its transcript."
					current={storage ? `${formatBytes(storage.audio_bytes)} now` : ""}
					policy={audioRetention}
					onChange={setAudioRetention}
				/>
			</div>

			<label className="mt-4 flex items-start gap-2.5 cursor-pointer">
				<input
					type="checkbox"
					checked={saveAudio}
					onChange={(e) => setSaveAudio(e.target.checked)}
					className="mt-0.5 shrink-0 rounded border-border accent-accent"
				/>
				<span className="min-w-0">
					<span className="text-[12.5px] text-text-primary">Keep audio at all</span>
					<span className="text-[11.5px] text-text-muted">
						{" "}
						— off means new records store the transcript only, and no audio retention applies.
					</span>
				</span>
			</label>

			<p className="mt-3 text-[11.5px] leading-relaxed text-text-muted">
				Evaluation samples are untouched by both policies. A sample owns its own copy of the audio,
				which is the point of it.
			</p>
		</Card>
	);
}
