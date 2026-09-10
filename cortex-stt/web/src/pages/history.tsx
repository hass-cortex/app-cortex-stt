import { HistoryList } from "@/components/history/history-list";
import { useSettings } from "@/hooks/use-settings";
import { useMetrics, useStorageInfo } from "@/hooks/use-system";
import { formatBytes, formatNumber } from "@/lib/format";
import { describeRetention } from "@/lib/retention";

export function HistoryPage() {
	const { data: metrics } = useMetrics();
	const { data: storage } = useStorageInfo();
	const { data: settings } = useSettings();

	return (
		<div className="flex flex-col gap-4">
			<div>
				<h1 className="text-[21px] font-semibold tracking-[-0.01em] text-text-primary">History</h1>
				<p className="text-[12.5px] text-text-secondary mt-1">
					{formatNumber(metrics?.total_transcriptions ?? 0)} records
					{storage ? ` · ${formatBytes(storage.audio_bytes)} audio` : ""}
					{settings
						? ` · rows ${describeRetention(settings.record_retention)}, audio ${describeRetention(settings.audio_retention)}`
						: ""}
				</p>
			</div>
			<HistoryList />
		</div>
	);
}
