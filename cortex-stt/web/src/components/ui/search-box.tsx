import { Search } from "lucide-react";

interface SearchBoxProps {
	value: string;
	onChange: (value: string) => void;
	placeholder?: string;
	/** Sizing and spacing belong to the row this sits in, not to the box. */
	className?: string;
}

/** A text filter over a list that is already on screen. */
export function SearchBox({ value, onChange, placeholder, className = "" }: SearchBoxProps) {
	return (
		<div
			className={`flex items-center gap-2.5 h-9 px-3 bg-surface-2 border border-border rounded-lg ${className}`}
		>
			<Search size={15} strokeWidth={1.8} className="text-text-muted shrink-0" />
			<input
				value={value}
				onChange={(e) => onChange(e.target.value)}
				placeholder={placeholder}
				autoComplete="off"
				className="w-full bg-transparent text-[12.5px] text-text-primary placeholder:text-text-muted focus:outline-none"
			/>
		</div>
	);
}
