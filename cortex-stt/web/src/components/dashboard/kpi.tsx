interface KpiProps {
	label: string;
	value: string;
	unit?: string;
	sub?: string;
	tone?: "default" | "error";
}

/** One headline number. The label is set in the mono face so a row of
 *  them lines up; the value is the only thing sized to be read across
 *  the room. */
export function Kpi({ label, value, unit, sub, tone = "default" }: KpiProps) {
	return (
		<div className="bg-surface-2 border border-border rounded-[10px] px-4 py-3.5">
			<div className="num text-[10.5px] tracking-[0.08em] text-text-muted">{label}</div>
			<div className="num mt-2 flex items-baseline gap-1.5">
				<span
					className={`text-[30px] font-bold tracking-[-0.02em] leading-none ${
						tone === "error" ? "text-error" : "text-text-primary"
					}`}
				>
					{value}
				</span>
				{unit && <span className="text-[13px] text-text-secondary">{unit}</span>}
			</div>
			{sub && <div className="num mt-[7px] text-[11px] text-text-faint">{sub}</div>}
		</div>
	);
}
