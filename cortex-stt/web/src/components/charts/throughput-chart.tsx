import { useRef } from "react";
import { ChartTooltip } from "@/components/charts/chart-tooltip";
import { useChartCursor } from "@/components/charts/use-chart-cursor";
import type { HourBucket } from "@/lib/stats";

interface ThroughputChartProps {
	buckets: HourBucket[];
	height?: number;
}

function hourLabel(date: Date): string {
	return String(date.getHours()).padStart(2, "0");
}

/**
 * Transcriptions per hour. One series, so no legend — the card title
 * names it. Bars are laid out in flow rather than in a fixed-width SVG so
 * the chart survives any container width.
 */
export function ThroughputChart({ buckets, height = 132 }: ThroughputChartProps) {
	const plot = useRef<HTMLDivElement>(null);
	const { active: activeIndex, barProps } = useChartCursor(plot);
	const max = Math.max(1, ...buckets.map((b) => b.count));
	const active = activeIndex === null ? null : (buckets[activeIndex] ?? null);

	return (
		<div className="w-full">
			<div className="flex gap-2">
				{/* y axis */}
				<div
					className="num flex flex-col justify-between text-[10px] text-text-faint shrink-0"
					style={{ height }}
				>
					<span>{max}</span>
					<span>0</span>
				</div>

				<div ref={plot} className="relative flex-1 min-w-0" style={{ height }}>
					<div className="absolute inset-x-0 top-0 border-t border-chart-grid" />
					<div className="absolute inset-x-0 top-1/2 border-t border-chart-grid" />
					<div className="absolute inset-x-0 bottom-0 border-t border-chart-axis" />

					<div className="absolute inset-0 flex items-end gap-[2px]">
						{buckets.map((bucket, index) => (
							<button
								type="button"
								key={bucket.start.toISOString()}
								className="relative flex-1 h-full flex items-end cursor-pointer"
								aria-label={`${hourLabel(bucket.start)}:00 — ${bucket.count} transcriptions`}
								{...barProps(index)}
							>
								{activeIndex === index && (
									<div className="absolute inset-y-0 left-1/2 w-px bg-accent-ink/60" />
								)}
								<div
									className="relative w-full rounded-[4px] bg-accent transition-[height] duration-300"
									style={{ height: `${(bucket.count / max) * 100}%` }}
								/>
							</button>
						))}
					</div>

					{active && activeIndex !== null && (
						<ChartTooltip index={activeIndex} count={buckets.length}>
							<div className="num text-[11px] text-text-primary whitespace-nowrap">
								{hourLabel(active.start)}:00 — {active.count}{" "}
								{active.count === 1 ? "request" : "requests"}
							</div>
							<div className="num text-[10.5px] text-text-muted whitespace-nowrap">
								{active.p50 === null ? "no timings" : `p50 ${active.p50} ms`}
							</div>
						</ChartTooltip>
					)}
				</div>
			</div>

			<div className="num flex justify-between text-[10px] text-text-faint mt-1.5 pl-7">
				{buckets
					.filter((_, i) => i % 6 === 0 || i === buckets.length - 1)
					.map((bucket) => (
						<span key={bucket.start.toISOString()}>{hourLabel(bucket.start)}</span>
					))}
			</div>
		</div>
	);
}
