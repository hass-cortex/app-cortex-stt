import type { InputHTMLAttributes } from "react";

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
	label?: string;
	error?: string;
}

export function Input({ label, error, id, className = "", ...props }: InputProps) {
	const inputId = id ?? label?.toLowerCase().replace(/\s+/g, "-");
	return (
		<div className="space-y-[6px]">
			{label && (
				<label
					htmlFor={inputId}
					className="num block text-[10.5px] uppercase tracking-[0.07em] text-text-muted"
				>
					{label}
				</label>
			)}
			<input
				id={inputId}
				className={`w-full h-[34px] px-[11px] text-[12.5px] bg-surface-3 border border-border rounded-[7px] text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-2 focus:ring-accent/40 focus:border-accent transition-colors ${
					error ? "border-error focus:ring-error/50" : ""
				} ${className}`}
				{...props}
			/>
			{error && <p className="text-xs text-error">{error}</p>}
		</div>
	);
}
