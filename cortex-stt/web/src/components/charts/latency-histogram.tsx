import { useRef } from "react";
import { ChartTooltip } from "@/components/charts/chart-tooltip";
import { useChartCursor } from "@/components/charts/use-chart-cursor";
import { formatDuration } from "@/lib/format";
import type { LatencySummary } from "@/lib/stats";

interface LatencyHistogramProps {
	summary: LatencySummary;
	height?: number;
}

/**
 * Distribution of inference time, with the median marked. A single hue:
 * the bins are one series, and shading the tail differently would encode
 * rank as colour, which it is not.
 */
export function LatencyHistogram({ summary, height = 96 }: LatencyHistogramProps) {
	const plot = useRef<HTMLDivElement>(null);
	const { active: activeIndex, barProps } = useChartCursor(plot);
	const { bins, p50, min, binMax } = summary;
	const max = Math.max(1, ...bins.map((b) => b.count));
	const p50Left = p50 === null ? null : ((p50 - min) / (binMax - min)) * 100;
	const active = activeIndex === null ? null : (bins[activeIndex] ?? null);

	if (bins.length === 0) {
		return (
			<div
				className="flex items-center justify-center text-[11.5px] text-text-muted"
				style={{ height }}
			>
				No completed requests in this window.
			</div>
		);
	}

	return (
		<div className="w-full">
			{/* The marker label gets its own lane: printed inside the plot it
			    lands on top of the tallest bar, which is exactly where the
			    median usually is. */}
			<div className="relative h-4">
				{p50Left !== null && p50Left >= 0 && p50Left <= 100 && (
					<span
						className="num absolute top-0 text-[10px] text-text-secondary whitespace-nowrap"
						style={{
							left: `${p50Left}%`,
							transform: p50Left > 70 ? "translateX(-100%)" : "translateX(-2px)",
						}}
					>
						p50 {formatDuration(p50 ?? 0)}
					</span>
				)}
			</div>
			<div ref={plot} className="relative" style={{ height }}>
				<div className="absolute inset-0 flex items-end gap-[2px]">
					{bins.map((bin, index) => (
						<button
							type="button"
							key={bin.from}
							className="relative flex-1 h-full flex items-end cursor-pointer"
							aria-label={`${Math.round(bin.from)}–${Math.round(bin.to)} ms — ${bin.count} requests`}
							{...barProps(index)}
						>
							{activeIndex === index && (
								<div className="absolute inset-y-0 left-1/2 w-px bg-accent-ink/60" />
							)}
							<div
								className="w-full rounded-[4px] bg-accent"
								style={{ height: `${Math.max(bin.count === 0 ? 0 : 3, (bin.count / max) * 100)}%` }}
							/>
						</button>
					))}
				</div>
				<div className="absolute inset-x-0 bottom-0 border-t border-chart-axis" />
				{p50Left !== null && p50Left >= 0 && p50Left <= 100 && (
					<div
						className="absolute top-0 bottom-0 border-l border-dashed border-text-primary/70"
						style={{ left: `${p50Left}%` }}
					/>
				)}
				{active && activeIndex !== null && (
					<ChartTooltip index={activeIndex} count={bins.length}>
						<div className="num text-[11px] text-text-primary whitespace-nowrap">
							{Math.round(active.from)}–{Math.round(active.to)} ms
						</div>
						<div className="num text-[10.5px] text-text-muted whitespace-nowrap">
							{active.count} {active.count === 1 ? "request" : "requests"}
						</div>
					</ChartTooltip>
				)}
			</div>
			<div className="num flex justify-between text-[10px] text-text-faint mt-1.5">
				<span>{Math.round(min)}</span>
				<span>{Math.round((min + binMax) / 2)}</span>
				<span>{Math.round(binMax)} ms</span>
			</div>
		</div>
	);
}
