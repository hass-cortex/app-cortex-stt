import type { ReactNode } from "react";
import type { TranscriptionRecord } from "@/api/types";
import { isQuietAndSilent } from "@/lib/capture-level";
import { formatDuration } from "@/lib/format";
import { formatClock } from "@/utils/time";

/** Shared by every surface that lists records, so the dashboard preview
 *  and the history page are the same row rather than two that drift. */
export const RECORD_ROW_CLASS =
	"w-full flex items-center gap-3.5 px-3 py-2.5 text-left cursor-pointer hover:bg-surface-3/40 transition-colors";

const headCell = "num text-[10px] tracking-[0.06em] text-text-faint";

/**
 * `select` is the bulk-selection tick box, when the list has one. The
 * dashboard's recent list does not, and passing nothing keeps its
 * columns exactly where they were.
 *
 * `actions` cover the headings rather than sitting above them, so the
 * header keeps the height its headings give it and no row moves when a
 * selection appears. The cover starts after `select`: covering the tick
 * box would need a second copy underneath it.
 */
export function RecordHead({ select, actions }: { select?: ReactNode; actions?: ReactNode }) {
	return (
		<div className="flex items-center gap-3.5 px-3 py-2 bg-surface-1 border-b border-border">
			{select}
			<div className="relative flex-1 min-w-0 flex items-center gap-3.5">
				{actions && (
					<div className="absolute inset-0 z-10 flex items-center gap-3 bg-surface-1">
						{actions}
					</div>
				)}
				<span className="w-3.5" />
				<span className={`${headCell} w-[46px]`}>TIME</span>
				<span className={`${headCell} flex-1`}>TRANSCRIPT</span>
				<span className={`${headCell} w-24 hidden lg:block`}>CAPTURE</span>
				<span className={`${headCell} w-36 hidden lg:block`}>MODEL</span>
				<span className={`${headCell} w-[52px] text-right hidden sm:block`}>AUDIO</span>
				<span className={`${headCell} w-[60px] text-right`}>INFER</span>
				<span className={`${headCell} w-11 text-right hidden sm:block`}>RTF</span>
			</div>
		</div>
	);
}

export function rtfOf(record: TranscriptionRecord): string {
	if (record.audio_duration_ms <= 0) return "—";
	return `${(record.inference_ms / record.audio_duration_ms).toFixed(2)}×`;
}

/** The cells only — the caller owns the row element, because a row is a
 *  disclosure button on the history page and a link on the dashboard.
 *
 *  `mark` rides alongside the transcript. History uses it to say a
 *  recording is already in the evaluation set, which is the one thing
 *  about a row that lives outside the record itself; the dashboard
 *  passes nothing. */
export function RecordCells({
	record,
	timezone,
	mark,
}: {
	record: TranscriptionRecord;
	timezone: string;
	mark?: ReactNode;
}) {
	const quiet = isQuietAndSilent(record);

	return (
		<>
			<span className="num w-[46px] text-[11px] text-text-faint shrink-0">
				{formatClock(record.timestamp, timezone)}
			</span>
			<span className="flex-1 min-w-0 flex items-center gap-2">
				<span className="min-w-0 truncate text-[13px] text-text-primary">
					{record.has_error ? (
						<span className="text-error">{record.error_message ?? "Failed"}</span>
					) : (
						record.text || <span className="text-text-muted italic">Empty</span>
					)}
				</span>
				{mark}
			</span>
			<span
				className={`num w-24 text-[11px] truncate hidden lg:block ${
					quiet ? "text-warning" : "text-text-muted"
				}`}
			>
				{record.capture_device ?? record.source}
			</span>
			<span className="num w-36 text-[11px] text-text-muted truncate hidden lg:block">
				{record.model_id}
			</span>
			<span className="num w-[52px] text-right text-[11px] text-text-muted hidden sm:block">
				{formatDuration(record.audio_duration_ms)}
			</span>
			<span className="num w-[60px] text-right text-[11px] text-text-primary">
				{formatDuration(record.inference_ms)}
			</span>
			<span className="num w-11 text-right text-[11px] text-text-faint hidden sm:block">
				{rtfOf(record)}
			</span>
		</>
	);
}
