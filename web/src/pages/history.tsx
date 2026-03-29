import { HistoryList } from "@/components/history/history-list";

export function HistoryPage() {
	return (
		<div className="space-y-6">
			<div>
				<h1 className="text-xl font-bold text-text-primary">History</h1>
				<p className="text-sm text-text-secondary mt-1">
					Browse transcription records, play audio, and inspect segments
				</p>
			</div>
			<HistoryList />
		</div>
	);
}
