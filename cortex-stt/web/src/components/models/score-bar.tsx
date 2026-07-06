interface ScoreBarProps {
	label: string;
	/** Score on a 0-100 scale, or null when unknown (renders nothing). */
	score: number | null;
	variant?: "accent" | "success" | "warning";
}

export function ScoreBar({ label, score, variant = "accent" }: ScoreBarProps) {
	if (score == null) return null;
	const percent = Math.max(0, Math.min(100, Math.round(score)));
	const colorMap = {
		accent: "bg-accent",
		success: "bg-success",
		warning: "bg-warning",
	};

	return (
		<div className="space-y-1">
			<div className="flex items-center justify-between">
				<span className="text-xs text-text-muted">{label}</span>
				<span className="text-xs font-medium text-text-secondary">{percent}%</span>
			</div>
			<div className="h-1.5 bg-surface-3 rounded-full overflow-hidden">
				<div
					className={`h-full rounded-full transition-all duration-300 ${colorMap[variant]}`}
					style={{ width: `${percent}%` }}
				/>
			</div>
		</div>
	);
}
