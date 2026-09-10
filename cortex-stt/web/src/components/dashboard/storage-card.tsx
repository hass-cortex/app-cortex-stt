import { Card, CardHeader } from "@/components/ui/card";
import { Spinner } from "@/components/ui/spinner";
import { useStorageInfo } from "@/hooks/use-system";
import { formatBytes } from "@/lib/format";

/** Models dominate this volume by three orders of magnitude, so the bar
 *  plots models against free space only — an audio segment sized to the
 *  same scale would be a fraction of a pixel wide and pretending
 *  otherwise would misstate it. The exact figures are in the list. */
export function StorageCard() {
	const { data, isLoading } = useStorageInfo();

	if (isLoading || !data) {
		return (
			<Card className="flex-1">
				<CardHeader title="Storage" />
				<div className="flex justify-center py-8">
					<Spinner />
				</div>
			</Card>
		);
	}

	const used = data.models_bytes + data.audio_bytes + data.database_bytes;
	const total = used + data.free_bytes;
	const modelsPercent = total > 0 ? (data.models_bytes / total) * 100 : 0;

	const rows = [
		{ label: "Models", bytes: data.models_bytes, swatch: "bg-accent" },
		{ label: "Audio", bytes: data.audio_bytes, swatch: "border border-warning" },
		{ label: "Database", bytes: data.database_bytes, swatch: "bg-surface-3" },
	];

	return (
		<Card className="flex-1">
			<CardHeader
				title="Storage"
				action={
					<span className="num text-[11px] text-text-muted">
						{formatBytes(data.free_bytes)} free
					</span>
				}
			/>
			<div className="mt-3 flex h-1.5 gap-0.5">
				<div className="bg-accent rounded-[3px]" style={{ width: `${modelsPercent}%` }} />
				<div className="flex-1 bg-surface-3 rounded-[3px]" />
			</div>
			<div className="mt-3 space-y-[7px]">
				{rows.map((row) => (
					<div key={row.label} className="flex items-center gap-2 text-[12px]">
						<span className={`w-[7px] h-[7px] rounded-[2px] shrink-0 ${row.swatch}`} />
						<span className="text-text-secondary">{row.label}</span>
						<span className="num ml-auto text-text-primary">{formatBytes(row.bytes)}</span>
					</div>
				))}
			</div>
		</Card>
	);
}
