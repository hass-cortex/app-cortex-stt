import { ArrowDown, ArrowUp, ChevronsUpDown } from "lucide-react";
import type { ReactNode } from "react";
import type { ActiveSort } from "@/hooks/use-table-sort";

interface SortControlProps {
	/** Key into the same column record the hook was given. */
	column: string;
	active: ActiveSort | null;
	onToggle: (key: string) => void;
	className?: string;
	children: ReactNode;
}

/** The direction this column is currently sorted in, if it is. */
function directionOf(active: ActiveSort | null, column: string) {
	return active?.key === column ? active.direction : null;
}

/**
 * A heading that sorts, without a cell around it — for lists that are
 * tabular in meaning but not built from `<table>`.
 *
 * The idle state carries a muted double chevron rather than revealing
 * one on hover: which columns sort has to be visible before the pointer
 * arrives, and on a touch screen there is no hover to reveal it.
 */
export function SortButton({
	column,
	active,
	onToggle,
	className = "",
	children,
}: SortControlProps) {
	const direction = directionOf(active, column);

	return (
		<button
			type="button"
			onClick={() => onToggle(column)}
			className={`inline-flex items-center gap-1 uppercase tracking-wider cursor-pointer transition-colors ${
				direction ? "text-text-primary" : "hover:text-text-secondary"
			} ${className}`}
		>
			{children}
			{direction === "asc" ? (
				<ArrowUp size={11} strokeWidth={2.2} className="text-accent" />
			) : direction === "desc" ? (
				<ArrowDown size={11} strokeWidth={2.2} className="text-accent" />
			) : (
				<ChevronsUpDown size={11} strokeWidth={1.8} className="text-text-faint" />
			)}
		</button>
	);
}

/** The same control as a table heading, carrying `aria-sort` on the cell. */
export function SortHeader({
	column,
	active,
	onToggle,
	className = "",
	children,
}: SortControlProps) {
	const direction = directionOf(active, column);

	return (
		<th
			className={`py-2 pr-4 font-medium ${className}`}
			aria-sort={direction === "asc" ? "ascending" : direction === "desc" ? "descending" : "none"}
		>
			<SortButton column={column} active={active} onToggle={onToggle}>
				{children}
			</SortButton>
		</th>
	);
}
