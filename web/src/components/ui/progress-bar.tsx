interface ProgressBarProps {
	/** 0 to 100 */
	value: number;
	/** Color variant */
	variant?: "accent" | "success" | "warning" | "error";
	/** Height */
	size?: "sm" | "md";
	/** Show percentage label */
	showLabel?: boolean;
	className?: string;
}

const variantBg: Record<string, string> = {
	accent: "bg-accent",
	success: "bg-success",
	warning: "bg-warning",
	error: "bg-error",
};

export function ProgressBar({
	value,
	variant = "accent",
	size = "md",
	showLabel = false,
	className = "",
}: ProgressBarProps) {
	const clamped = Math.min(100, Math.max(0, value));
	const height = size === "sm" ? "h-1.5" : "h-2.5";

	return (
		<div className={`flex items-center gap-2 ${className}`}>
			<div className={`flex-1 bg-surface-3 rounded-full overflow-hidden ${height}`}>
				<div
					className={`${height} rounded-full transition-all duration-300 ease-out ${variantBg[variant]}`}
					style={{ width: `${clamped}%` }}
				/>
			</div>
			{showLabel && (
				<span className="text-xs text-text-muted w-10 text-right">{Math.round(clamped)}%</span>
			)}
		</div>
	);
}
