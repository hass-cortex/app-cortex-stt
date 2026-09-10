import type { TranscriptionRecord } from "@/api/types";
import { useHistoryList } from "@/hooks/use-history";
import { formatDbfs, isQuiet } from "@/lib/capture-level";

const FLOOR_DBFS = -60;
const CEILING_DBFS = -6;
const BARS = 20;

/**
 * The level of the most recent capture, not a live microphone feed — the
 * server measures RMS per request and that is the signal a maintainer can
 * act on. Updates as records arrive over the history SSE stream.
 */
export function LiveInput() {
	const { data } = useHistoryList({ limit: 1 });
	const record: TranscriptionRecord | undefined = data?.[0];
	const rms = record?.rms_db ?? null;

	const filled =
		rms === null
			? 0
			: Math.round(
					((Math.min(CEILING_DBFS, Math.max(FLOOR_DBFS, rms)) - FLOOR_DBFS) /
						(CEILING_DBFS - FLOOR_DBFS)) *
						BARS,
				);
	const quiet = isQuiet(rms);

	return (
		<div className="hidden md:flex items-center gap-3.5 min-w-0">
			<span className="num text-[10.5px] tracking-[0.09em] text-text-muted shrink-0">
				LAST INPUT
			</span>
			<div className="flex items-end gap-[3px] h-[18px] shrink-0" aria-hidden="true">
				{Array.from({ length: BARS }, (_, i) => {
					const on = i < filled;
					const height = 4 + Math.round(Math.sin(((i + 1) / BARS) * Math.PI) * 12);
					return (
						<span
							// biome-ignore lint/suspicious/noArrayIndexKey: fixed-length meter; the index is the position
							key={i}
							className={`w-[3px] rounded-[1.5px] ${
								on ? (quiet ? "bg-warning" : "bg-accent") : "bg-border"
							}`}
							style={{ height }}
						/>
					);
				})}
			</div>
			<span
				className={`num text-[11px] truncate ${quiet ? "text-warning" : "text-text-secondary"}`}
			>
				{rms === null
					? "no level recorded"
					: `${formatDbfs(rms)}${record?.capture_device ? ` · ${record.capture_device}` : ""}`}
			</span>
		</div>
	);
}
