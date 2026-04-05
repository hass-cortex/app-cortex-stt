interface ScoreBarProps {
	label: string;
	score: number;
	variant?: "accent" | "success" | "warning";
}

export function ScoreBar({ label, score, variant = "accent" }: ScoreBarProps) {
	const percent = Math.round(score * 100);
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
