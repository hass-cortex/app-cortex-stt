import { useCallback, useMemo, useState } from "react";

export interface Selection {
	ids: Set<string>;
	size: number;
	has: (id: string) => boolean;
	toggle: (id: string) => void;
	/** Add every id given; leaves anything else selected as it was. */
	add: (ids: string[]) => void;
	/** Remove every id given; leaves anything else selected as it was. */
	remove: (ids: string[]) => void;
	clear: () => void;
}

/**
 * Which rows a bulk action applies to.
 *
 * A selection outlives the filter that made it, so the count is of
 * everything ticked, shown or not. Callers pass the shown ids in to work
 * out what reaches past the screen.
 */
export function useSelection(): Selection {
	const [ids, setIds] = useState<Set<string>>(new Set());

	const toggle = useCallback((id: string) => {
		setIds((prev) => {
			const next = new Set(prev);
			if (next.has(id)) next.delete(id);
			else next.add(id);
			return next;
		});
	}, []);

	const add = useCallback((given: string[]) => {
		setIds((prev) => new Set([...prev, ...given]));
	}, []);

	const remove = useCallback((given: string[]) => {
		setIds((prev) => {
			const next = new Set(prev);
			for (const id of given) next.delete(id);
			return next;
		});
	}, []);

	const clear = useCallback(() => setIds(new Set()), []);

	return useMemo(
		() => ({
			ids,
			size: ids.size,
			has: (id: string) => ids.has(id),
			toggle,
			add,
			remove,
			clear,
		}),
		[ids, toggle, add, remove, clear],
	);
}
