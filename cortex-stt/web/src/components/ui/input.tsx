import type { InputHTMLAttributes } from "react";

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
	label?: string;
	error?: string;
}

export function Input({ label, error, id, className = "", ...props }: InputProps) {
	const inputId = id ?? label?.toLowerCase().replace(/\s+/g, "-");
	return (
		<div className="space-y-1.5">
			{label && (
				<label htmlFor={inputId} className="block text-sm font-medium text-text-secondary">
					{label}
				</label>
			)}
			<input
				id={inputId}
				className={`w-full px-3 py-2 text-sm bg-surface-3 border border-border rounded-lg text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-2 focus:ring-accent/50 focus:border-accent transition-colors ${
					error ? "border-error focus:ring-error/50" : ""
				} ${className}`}
				{...props}
			/>
			{error && <p className="text-xs text-error">{error}</p>}
		</div>
	);
}
