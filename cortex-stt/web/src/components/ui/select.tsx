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
		<div className="space-y-1.5">
			{label && (
				<label htmlFor={selectId} className="block text-sm font-medium text-text-secondary">
					{label}
				</label>
			)}
			<select
				id={selectId}
				className={`w-full px-3 py-2 text-sm bg-surface-3 border border-border rounded-lg text-text-primary focus:outline-none focus:ring-2 focus:ring-accent/50 focus:border-accent transition-colors appearance-none cursor-pointer ${
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
