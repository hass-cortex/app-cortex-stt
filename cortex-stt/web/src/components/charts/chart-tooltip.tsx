import type { ReactNode } from "react";

/**
 * Beside the bar it belongs to, flipped once past the midpoint so it
 * never leaves the plot. Bars are laid out in flow at equal width, so the
 * bar's own position is all the geometry this needs.
 */
export function ChartTooltip({
	index,
	count,
	children,
}: {
	index: number;
	count: number;
	children: ReactNode;
}) {
	return (
		<div
			className="absolute z-10 pointer-events-none px-3 py-2 rounded-[6px] bg-surface-3 border border-border shadow-lg"
			style={{
				left: `${((index + 0.5) / count) * 100}%`,
				transform: index > count / 2 ? "translate(-100%, 0)" : "translate(12px, 0)",
				top: 4,
			}}
		>
			{children}
		</div>
	);
}
