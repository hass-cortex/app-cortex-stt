import type { TranscriptionSegment } from "@/api/types";

interface ClipTimelineProps {
	/** Normalised 0..1 peaks, one per drawn bar. */
	peaks: number[];
	durationMs: number;
	/** Segment boundaries in seconds. Rendered on the same axis as the
	 *  waveform, so an edge sits at its own timestamp. */
	segments?: TranscriptionSegment[];
	/** 0..1 — how much has played. */
	playedRatio?: number;
	onSeek?: (ratio: number) => void;
	/** Off when there is no audio behind the axis — the segments still
	 *  carry their timings, but nothing is playing along them. */
	playhead?: boolean;
	height?: number;
	className?: string;
}

function clockLabel(seconds: number): string {
	return seconds.toFixed(2);
}

/** Above this share of the clip, a lone segment is "the whole thing". */
const WHOLE_CLIP = 0.98;

/**
 * Whether the segmentation says anything the transcript does not.
 *
 * A single segment spanning the clip divides nothing: the strip would
 * repeat the transcript inside a box the width of the axis. Exported so a
 * caller can drop the surrounding block on the same rule.
 */
export function segmentsDivideClip(segments: TranscriptionSegment[], durationMs: number): boolean {
	if (!segments.some((s) => s.end > s.start)) return false;
	const only = segments.length === 1 ? segments[0] : undefined;
	if (!only) return true;
	return durationMs <= 0 || (only.end - only.start) * 1000 < durationMs * WHOLE_CLIP;
}

/**
 * Waveform, segments and time ticks on one axis.
 *
 * Everything is laid out as a percentage of the clip's duration, so the
 * three layers stay registered at any container width: a segment's left
 * edge, its tick and the sample it starts on are the same x. Widths are
 * border-box with the divider drawn as a border, so N segments still sum
 * to exactly 100% and the boundaries cannot drift.
 */
export function ClipTimeline({
	peaks,
	durationMs,
	segments = [],
	playedRatio = 0,
	onSeek,
	playhead = true,
	height = 40,
	className = "",
}: ClipTimelineProps) {
	const duration = durationMs / 1000;
	const pct = (seconds: number) => (duration > 0 ? (seconds / duration) * 100 : 0);

	// Some families return segments with no timings at all (start === end).
	// Those cannot be placed on the axis, and stretching them across it
	// would assert a timing the model never gave.
	const timed = segments.some((s) => s.end > s.start);
	const divided = segmentsDivideClip(segments, durationMs);

	// Gaps between segments are real (silence the model did not label);
	// spacers keep the strip on the same axis instead of closing them up.
	const pieces: { key: string; width: number; segment: TranscriptionSegment | null }[] = [];
	let cursor = 0;
	for (const [index, segment] of (divided ? segments : []).entries()) {
		if (segment.start > cursor + 0.001) {
			pieces.push({ key: `gap-${index}`, width: pct(segment.start - cursor), segment: null });
		}
		pieces.push({
			key: `seg-${index}`,
			width: pct(Math.max(0, segment.end - segment.start)),
			segment,
		});
		cursor = Math.max(cursor, segment.end);
	}
	if (divided && duration - cursor > 0.001) {
		pieces.push({ key: "gap-tail", width: pct(duration - cursor), segment: null });
	}

	// With no strip the axis still needs its two ends labelled.
	const boundaries = divided ? [0, ...segments.map((s) => s.end)] : [0, duration];

	return (
		<div className={`relative w-full ${className}`}>
			{/* Only a control when there is something to seek: with no audio
			    behind it this is a picture of the clip, not a transport. */}
			{onSeek ? (
				<button
					type="button"
					className="block w-full cursor-pointer"
					style={{ height }}
					onClick={(event) => {
						const box = event.currentTarget.getBoundingClientRect();
						onSeek(Math.min(1, Math.max(0, (event.clientX - box.left) / box.width)));
					}}
					aria-label="Seek"
				>
					<span className="flex items-center gap-px w-full" style={{ height }}>
						{peaks.length === 0 && <span className="flex-1 h-[2px] rounded-full bg-accent-quiet" />}
						{peaks.map((peak, index) => (
							<span
								// biome-ignore lint/suspicious/noArrayIndexKey: a bar's identity IS its position on the time axis
								key={index}
								className={`flex-1 min-w-px rounded-[1.5px] ${
									index / peaks.length < playedRatio ? "bg-accent" : "bg-accent-quiet"
								}`}
								style={{ height: `${Math.max(6, peak * 100)}%` }}
							/>
						))}
					</span>
				</button>
			) : (
				<div className="block w-full" style={{ height }}>
					<span className="flex items-center gap-px w-full" style={{ height }}>
						{peaks.length === 0 && <span className="flex-1 h-[2px] rounded-full bg-accent-quiet" />}
						{peaks.map((peak, index) => (
							<span
								// biome-ignore lint/suspicious/noArrayIndexKey: a bar's identity IS its position on the time axis
								key={index}
								className={`flex-1 min-w-px rounded-[1.5px] ${
									index / peaks.length < playedRatio ? "bg-accent" : "bg-accent-quiet"
								}`}
								style={{ height: `${Math.max(6, peak * 100)}%` }}
							/>
						))}
					</span>
				</div>
			)}

			{pieces.length > 0 && (
				<div className="flex mt-1">
					{pieces.map((piece, index) => (
						<div
							key={piece.key}
							className={`box-border min-w-0 ${
								piece.segment
									? "px-2 py-1.5 bg-surface-2 border-l-2 border-l-accent rounded-r-[5px]"
									: ""
							}`}
							style={{
								width: `${piece.width}%`,
								borderRight: index === pieces.length - 1 ? undefined : "2px solid transparent",
							}}
						>
							{piece.segment &&
								(onSeek ? (
									<button
										type="button"
										className="w-full text-left cursor-pointer"
										onClick={() => onSeek(pct(piece.segment?.start ?? 0) / 100)}
									>
										<div className="num text-[10px] text-text-faint">
											{clockLabel(piece.segment.start)}
										</div>
										<div className="text-[12px] text-text-primary truncate">
											{piece.segment.text.trim()}
										</div>
									</button>
								) : (
									<>
										<div className="num text-[10px] text-text-faint">
											{clockLabel(piece.segment.start)}
										</div>
										<div className="text-[12px] text-text-primary truncate">
											{piece.segment.text.trim()}
										</div>
									</>
								))}
						</div>
					))}
				</div>
			)}

			{/* Untimed segments cannot go on the axis, so all this strip can
			    carry is the split itself — and one segment is not a split: its
			    text is the transcript, already on screen under this. */}
			{segments.length > 1 && !timed && (
				<div className="mt-1.5 px-2 py-1.5 bg-surface-2 border-l-2 border-l-accent-quiet rounded-r-[5px]">
					<div className="num text-[10px] text-text-faint">no segment timings reported</div>
					<div className="text-[12px] text-text-primary truncate">
						{segments.map((s) => s.text.trim()).join(" ")}
					</div>
				</div>
			)}

			<div className="relative h-4 mt-1">
				{boundaries.map((seconds, index) => (
					<span
						key={seconds}
						className="absolute top-0 flex flex-col"
						style={{
							left: `${pct(seconds)}%`,
							transform:
								index === 0
									? "none"
									: index === boundaries.length - 1
										? "translateX(-100%)"
										: "translateX(-50%)",
							alignItems:
								index === 0
									? "flex-start"
									: index === boundaries.length - 1
										? "flex-end"
										: "center",
						}}
					>
						<span className="w-px h-1 bg-border" />
						<span className="num mt-0.5 text-[9.5px] text-text-faint whitespace-nowrap">
							{clockLabel(seconds)}
						</span>
					</span>
				))}
			</div>

			{playhead && (
				<div
					className="absolute top-0 w-px bg-text-primary pointer-events-none"
					style={{ left: `${playedRatio * 100}%`, height: height + (pieces.length ? 0 : 0) }}
				>
					<span className="absolute -left-[3px] -top-[3px] w-[7px] h-[7px] rounded-full bg-text-primary" />
				</div>
			)}
		</div>
	);
}
