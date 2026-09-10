import { type RefObject, useEffect, useRef } from "react";

/**
 * Escape, a press outside, or a resize all mean "done reading".
 *
 * Anything opened by a tap has to be closable by a tap: a touch device
 * never fires the pointer-leave that closes a hover, so without this an
 * overlay opened on a phone would stay open until the element that owns
 * it unmounts.
 */
export function useDismiss(
	active: boolean,
	dismiss: () => void,
	inside: RefObject<HTMLElement | null>[],
): void {
	// Read at event time, not at subscribe time: the caller rebuilds the
	// array every render, and re-subscribing on every render would drop
	// the listener between the press and the click it belongs to.
	const insideRef = useRef(inside);
	insideRef.current = inside;

	useEffect(() => {
		if (!active) return;

		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") dismiss();
		};
		// Capture phase: a press on a control that stops propagation is
		// still a press somewhere else.
		const onPress = (e: PointerEvent) => {
			const target = e.target as Node;
			if (insideRef.current.some((ref) => ref.current?.contains(target))) return;
			dismiss();
		};

		window.addEventListener("keydown", onKey);
		window.addEventListener("pointerdown", onPress, true);
		window.addEventListener("resize", dismiss);
		return () => {
			window.removeEventListener("keydown", onKey);
			window.removeEventListener("pointerdown", onPress, true);
			window.removeEventListener("resize", dismiss);
		};
	}, [active, dismiss]);
}
