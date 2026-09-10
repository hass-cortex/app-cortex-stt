import { ChevronRight } from "lucide-react";
import { Link } from "react-router";
import type { TranscriptionRecord } from "@/api/types";
import { RECORD_ROW_CLASS, RecordCells, RecordHead } from "@/components/transcript/record-row";
import { Card, CardHeader } from "@/components/ui/card";
import { useSettings } from "@/hooks/use-settings";
import { ROUTES } from "@/lib/constants";

interface RecentCardProps {
	records: TranscriptionRecord[];
}

export function RecentCard({ records }: RecentCardProps) {
	const { data: settings } = useSettings();
	const timezone = settings?.timezone ?? "auto";

	return (
		<Card padding="none" className="flex-1 min-w-0 flex flex-col overflow-hidden">
			<div className="px-[18px] pt-4 pb-3">
				<CardHeader
					title="Recent transcriptions"
					action={
						<Link to={ROUTES.HISTORY} className="text-[11.5px] text-accent-ink hover:underline">
							View all
						</Link>
					}
				/>
			</div>

			<RecordHead />

			{records.length === 0 ? (
				<p className="text-[12px] text-text-muted py-6 text-center">
					Nothing transcribed in the last 24 hours.
				</p>
			) : (
				records.map((record) => (
					<Link
						key={record.id}
						to={`${ROUTES.HISTORY}?record=${encodeURIComponent(record.id)}`}
						className={`${RECORD_ROW_CLASS} border-b border-border-soft last:border-0`}
					>
						<ChevronRight size={14} strokeWidth={1.8} className="text-text-faint shrink-0" />
						<RecordCells record={record} timezone={timezone} />
					</Link>
				))
			)}
		</Card>
	);
}
