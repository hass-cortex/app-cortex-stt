interface SegmentedOption<T extends string> {
	value: T;
	label: string;
}

interface SegmentedProps<T extends string> {
	options: SegmentedOption<T>[];
	value: T;
	onChange: (value: T) => void;
	className?: string;
}

/** A small set of mutually exclusive choices, shown in full. Use a Select
 *  instead once the list stops fitting on one line. */
export function Segmented<T extends string>({
	options,
	value,
	onChange,
	className = "",
}: SegmentedProps<T>) {
	return (
		<div
			className={`inline-flex p-0.5 bg-surface-3 border border-border rounded-[7px] ${className}`}
		>
			{options.map((option) => (
				<button
					key={option.value}
					type="button"
					onClick={() => onChange(option.value)}
					className={`num px-2.5 py-[5px] rounded-[5px] text-[11.5px] transition-colors cursor-pointer ${
						option.value === value
							? "bg-accent text-white"
							: "text-text-muted hover:text-text-primary"
					}`}
				>
					{option.label}
				</button>
			))}
		</div>
	);
}
