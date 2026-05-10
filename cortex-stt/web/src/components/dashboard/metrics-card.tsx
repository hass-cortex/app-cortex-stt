import { BarChart3, Clock, Mic, XCircle } from "lucide-react";
import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useMetrics } from "@/hooks/use-system";
import { formatDuration, formatNumber } from "@/lib/format";

interface StatItemProps {
	icon: typeof Mic;
	label: string;
	value: string;
	subValue?: string;
	iconClass?: string;
}

function StatItem({
	icon: Icon,
	label,
	value,
	subValue,
	iconClass = "text-text-muted",
}: StatItemProps) {
	return (
		<div className="flex items-center gap-3">
			<div className={`p-2 rounded-lg bg-surface-3 ${iconClass}`}>
				<Icon size={16} />
			</div>
			<div>
				<p className="text-lg font-semibold text-text-primary">{value}</p>
				<p className="text-xs text-text-muted">{label}</p>
				{subValue && <p className="text-xs text-text-secondary">{subValue}</p>}
			</div>
		</div>
	);
}

export function MetricsCard() {
	const { data, isLoading } = useMetrics();

	if (isLoading) {
		return (
			<Card>
				<CardHeader title="Today's Metrics" />
				<div className="flex justify-center py-8">
					<Spinner />
				</div>
			</Card>
		);
	}

	if (!data) return null;

	return (
		<Card>
			<CardHeader title="Today's Metrics" />
			<div className="grid grid-cols-2 gap-4">
				<StatItem
					icon={Mic}
					label="Transcriptions"
					value={formatNumber(data.today_transcriptions)}
					subValue={`${formatNumber(data.total_transcriptions)} total`}
				/>
				<StatItem icon={Clock} label="Avg. Latency" value={formatDuration(data.avg_inference_ms)} />
				<StatItem
					icon={BarChart3}
					label="Audio Processed"
					value={formatDuration(data.today_audio_duration_ms)}
					subValue={`${formatDuration(data.total_audio_duration_ms)} total`}
				/>
				<StatItem
					icon={XCircle}
					label="Errors"
					value={formatNumber(data.today_error_count)}
					subValue={`${formatNumber(data.error_count)} total`}
					iconClass={data.today_error_count > 0 ? "text-error" : "text-text-muted"}
				/>
			</div>
		</Card>
	);
}
