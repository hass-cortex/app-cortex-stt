import { type PointerEvent, type RefObject, useCallback, useState } from "react";
import { useDismiss } from "@/hooks/use-dismiss";

/**
 * Which bar the reader is asking about.
 *
 * A mouse answers by hovering and a phone has no hover, so a tap pins a
 * bar instead — and a pinned bar outranks a hovered one, so the two
 * never fight over the same tooltip.
 */
export function useChartCursor(container: RefObject<HTMLElement | null>) {
	const [hovered, setHovered] = useState<number | null>(null);
	const [pinned, setPinned] = useState<number | null>(null);

	const clear = useCallback(() => {
		setPinned(null);
		setHovered(null);
	}, []);

	useDismiss(pinned !== null, clear, [container]);

	const barProps = useCallback(
		(index: number) => ({
			onPointerEnter: (e: PointerEvent) => {
				if (e.pointerType === "mouse") setHovered(index);
			},
			onPointerLeave: (e: PointerEvent) => {
				if (e.pointerType === "mouse") setHovered(null);
			},
			onFocus: () => setHovered(index),
			onBlur: () => setHovered(null),
			onClick: () => setPinned((p) => (p === index ? null : index)),
		}),
		[],
	);

	return { active: pinned ?? hovered, barProps, clear };
}
