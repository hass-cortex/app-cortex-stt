import { useMemo, useState } from "react";

export type SortDirection = "asc" | "desc";

export interface ActiveSort {
	key: string;
	direction: SortDirection;
}

/** What one column contributes to the ordering. */
export interface SortColumn<T> {
	/** The comparable value, or null when the row has no measurement for
	 *  this column — a model that never loaded is not the fastest one. */
	value: (row: T) => number | string | null;
	/** Which end the first click brings to the top. A latency column reads
	 *  best-first ascending and a score descending, so the direction
	 *  belongs to the column rather than to the table. */
	first: SortDirection;
}

export interface TableSort<T> {
	rows: T[];
	active: ActiveSort | null;
	toggle: (key: string) => void;
	/** Back to the order the caller passed in, which usually means
	 *  something of its own. */
	reset: () => void;
}

/**
 * The ordering itself, kept out of the hook so it can be reasoned about
 * (and exercised) without a render.
 */
export function sortRows<T>(rows: T[], column: SortColumn<T>, direction: SortDirection): T[] {
	const sign = direction === "asc" ? 1 : -1;
	// Stable, so rows this column cannot separate keep the order they
	// arrived in instead of shuffling on every re-sort.
	return [...rows].sort((a, b) => {
		const left = column.value(a);
		const right = column.value(b);
		// Missing sinks in both directions rather than surfacing at the top
		// the moment the sort reverses.
		if (left === null || right === null) return left === right ? 0 : left === null ? 1 : -1;
		if (typeof left === "string" || typeof right === "string") {
			return sign * String(left).localeCompare(String(right));
		}
		return sign * (left - right);
	});
}

/**
 * Sorting as a view state, over rows whose given order still means
 * something.
 *
 * Define `columns` at module scope: the extractors are pure functions of
 * a row, and a literal rebuilt each render would defeat the memo.
 */
export function useTableSort<T>(rows: T[], columns: Record<string, SortColumn<T>>): TableSort<T> {
	const [active, setActive] = useState<ActiveSort | null>(null);

	const sorted = useMemo(() => {
		const column = active && columns[active.key];
		return active && column ? sortRows(rows, column, active.direction) : rows;
	}, [rows, columns, active]);

	return {
		rows: sorted,
		active,
		toggle: (key: string) =>
			setActive((current) =>
				current?.key === key
					? { key, direction: current.direction === "asc" ? "desc" : "asc" }
					: { key, direction: columns[key]?.first ?? "asc" },
			),
		reset: () => setActive(null),
	};
}
