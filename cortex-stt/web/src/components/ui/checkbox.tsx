import { Check, Minus } from "lucide-react";
import { useEffect, useRef } from "react";

interface CheckboxProps {
	checked: boolean;
	/** Some of the rows this stands for, but not all. */
	indeterminate?: boolean;
	onChange: () => void;
	/** What ticking this selects. Read out instead of a visible caption,
	 *  because the row beside it already says which one this is. */
	label: string;
	className?: string;
}

/**
 * A tick box for choosing rows.
 *
 * The native control is kept and hidden rather than replaced by a
 * `<div role="checkbox">`: the label, the keyboard, and the form
 * semantics come free, and only the box itself is drawn.
 */
export function Checkbox({
	checked,
	indeterminate = false,
	onChange,
	label,
	className = "",
}: CheckboxProps) {
	// `indeterminate` has no attribute — it exists only as a property, so
	// without this the box draws a dash while screen readers say
	// "unchecked".
	const ref = useRef<HTMLInputElement>(null);
	useEffect(() => {
		if (ref.current) ref.current.indeterminate = indeterminate && !checked;
	}, [indeterminate, checked]);

	return (
		<label className={`inline-flex shrink-0 items-center cursor-pointer ${className}`}>
			<input
				ref={ref}
				type="checkbox"
				checked={checked}
				onChange={onChange}
				aria-label={label}
				className="sr-only peer"
			/>
			<span
				aria-hidden="true"
				className={`w-[15px] h-[15px] rounded-[4px] border flex items-center justify-center transition-colors peer-focus-visible:ring-2 peer-focus-visible:ring-accent/50 ${
					checked || indeterminate
						? "bg-accent border-accent text-white"
						: "border-border bg-surface-2 text-transparent hover:border-text-muted"
				}`}
			>
				{indeterminate && !checked ? (
					<Minus size={11} strokeWidth={3} />
				) : (
					<Check size={11} strokeWidth={3} />
				)}
			</span>
		</label>
	);
}
