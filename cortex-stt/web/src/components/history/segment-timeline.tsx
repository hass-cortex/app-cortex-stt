import type { TranscriptionSegment } from "@/api/types";

interface SegmentTimelineProps {
	segments: TranscriptionSegment[];
	totalDurationMs: number;
}

export function SegmentTimeline({ segments, totalDurationMs }: SegmentTimelineProps) {
	const totalSec = totalDurationMs / 1000;
	if (segments.length === 0 || totalSec <= 0) return null;

	return (
		<div className="space-y-2">
			<h4 className="text-xs font-medium text-text-secondary uppercase tracking-wider">Segments</h4>

			{/* Visual timeline */}
			<div className="relative h-6 bg-surface-3 rounded overflow-hidden">
				{segments.map((seg) => {
					const left = (seg.start / totalSec) * 100;
					const width = ((seg.end - seg.start) / totalSec) * 100;
					return (
						<div
							key={`${seg.start}-${seg.end}`}
							className="absolute top-0 h-full bg-accent/30 border-l border-accent/60"
							style={{ left: `${left}%`, width: `${Math.max(width, 0.5)}%` }}
							title={`${seg.start.toFixed(1)}s - ${seg.end.toFixed(1)}s: ${seg.text}`}
						/>
					);
				})}
			</div>

			{/* Segment list */}
			<div className="space-y-1 max-h-48 overflow-y-auto">
				{segments.map((seg) => (
					<div
						key={`${seg.start}-${seg.end}`}
						className="flex items-start gap-3 text-xs py-1.5 px-2 rounded hover:bg-surface-3 transition-colors"
					>
						<span className="text-text-muted font-mono shrink-0 w-24">
							{seg.start.toFixed(1)}s - {seg.end.toFixed(1)}s
						</span>
						<span className="text-text-primary">{seg.text}</span>
					</div>
				))}
			</div>
		</div>
	);
}
