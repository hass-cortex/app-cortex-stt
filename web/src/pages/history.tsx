import { HistoryDetail } from "@/components/history/history-detail";
import { HistoryList } from "@/components/history/history-list";
import { useState } from "react";

export function HistoryPage() {
	const [selectedId, setSelectedId] = useState<string | null>(null);

	if (selectedId) {
		return <HistoryDetail recordId={selectedId} onBack={() => setSelectedId(null)} />;
	}

	return (
		<div className="space-y-6">
			<div>
				<h1 className="text-xl font-bold text-text-primary">History</h1>
				<p className="text-sm text-text-secondary mt-1">
					Browse transcription records, play audio, and inspect segments
				</p>
			</div>
			<HistoryList onSelectRecord={setSelectedId} />
		</div>
	);
}
