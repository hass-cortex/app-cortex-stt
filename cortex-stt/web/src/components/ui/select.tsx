import type { SelectHTMLAttributes } from "react";

interface SelectOption {
	value: string;
	label: string;
	disabled?: boolean;
}

interface SelectProps extends SelectHTMLAttributes<HTMLSelectElement> {
	label?: string;
	options: SelectOption[];
	error?: string;
	placeholder?: string;
}

export function Select({
	label,
	options,
	error,
	placeholder,
	id,
	className = "",
	...props
}: SelectProps) {
	const selectId = id ?? label?.toLowerCase().replace(/\s+/g, "-");
	return (
		<div className="space-y-[6px]">
			{label && (
				<label
					htmlFor={selectId}
					className="num block text-[10.5px] uppercase tracking-[0.07em] text-text-muted"
				>
					{label}
				</label>
			)}
			<select
				id={selectId}
				className={`w-full h-[34px] px-[11px] text-[12.5px] bg-surface-3 border border-border rounded-[7px] text-text-primary focus:outline-none focus:ring-2 focus:ring-accent/40 focus:border-accent transition-colors cursor-pointer ${
					error ? "border-error" : ""
				} ${className}`}
				{...props}
			>
				{placeholder && (
					<option value="" disabled>
						{placeholder}
					</option>
				)}
				{options.map((opt) => (
					<option key={opt.value} value={opt.value} disabled={opt.disabled}>
						{opt.label}
					</option>
				))}
			</select>
			{error && <p className="text-xs text-error">{error}</p>}
		</div>
	);
}
