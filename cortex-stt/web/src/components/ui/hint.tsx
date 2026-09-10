import {
	type ReactNode,
	useCallback,
	useEffect,
	useId,
	useLayoutEffect,
	useRef,
	useState,
} from "react";
import { useDismiss } from "@/hooks/use-dismiss";

interface HintProps {
	/** The bubble's body — what the mark stands for. */
	content: ReactNode;
	/** Names the mark for assistive tech, and titles the bubble. */
	label?: string;
	children: ReactNode;
	className?: string;
	width?: number;
	/** Expand the tap target around a small mark. Off when a neighbouring
	 *  control sits close enough that the overlap would steal its taps. */
	hitPad?: boolean;
}

const GAP = 8;
const EDGE = 8;

/**
 * A mark you can read on any pointer.
 *
 * The native `title` is a desktop-only affordance: a touch device has no
 * hover, so an explanation carried in `title` alone is simply absent
 * there. Hover still opens the bubble for a mouse; a tap — or Enter on
 * the focused mark — pins it open, which is the only channel a phone has.
 */
export function Hint({
	content,
	label,
	children,
	className = "",
	width = 280,
	hitPad = true,
}: HintProps) {
	const id = useId();
	const trigger = useRef<HTMLButtonElement>(null);
	const bubble = useRef<HTMLDivElement>(null);
	const [hovered, setHovered] = useState(false);
	const [pinned, setPinned] = useState(false);
	const [place, setPlace] = useState<{ left: number; top: number } | null>(null);
	const open = hovered || pinned;

	const dismiss = useCallback(() => {
		setPinned(false);
		setHovered(false);
	}, []);
	useDismiss(pinned, dismiss, [trigger, bubble]);

	// The bubble is anchored to a rect measured once, so a scroll would
	// strand it beside where the mark used to be.
	useEffect(() => {
		if (!pinned) return;
		window.addEventListener("scroll", dismiss, true);
		return () => window.removeEventListener("scroll", dismiss, true);
	}, [pinned, dismiss]);

	// Measured after paint: whether the bubble sits below the mark or flips
	// above it depends on how its text wrapped at this viewport width.
	useLayoutEffect(() => {
		if (!open) {
			setPlace(null);
			return;
		}
		const anchor = trigger.current?.getBoundingClientRect();
		if (!anchor) return;
		const height = bubble.current?.getBoundingClientRect().height ?? 0;
		const below = anchor.bottom + GAP;
		const flip = below + height > window.innerHeight - EDGE && anchor.top - GAP - height > EDGE;
		setPlace({
			left: Math.min(
				Math.max(EDGE, anchor.left + anchor.width / 2 - width / 2),
				Math.max(EDGE, window.innerWidth - width - EDGE),
			),
			top: flip ? anchor.top - GAP - height : below,
		});
	}, [open, width]);

	return (
		<>
			<button
				ref={trigger}
				type="button"
				aria-label={label}
				aria-expanded={open}
				aria-describedby={open ? id : undefined}
				onClick={() => setPinned((p) => !p)}
				onPointerEnter={(e) => e.pointerType === "mouse" && setHovered(true)}
				onPointerLeave={(e) => e.pointerType === "mouse" && setHovered(false)}
				// The mark is small by design; the tap target around it is not.
				className={`relative inline-flex items-center cursor-pointer ${
					hitPad ? "after:absolute after:-inset-2.5 after:content-['']" : ""
				} ${className}`}
			>
				{children}
			</button>
			{open && (
				<div
					ref={bubble}
					id={id}
					role="tooltip"
					className="fixed z-50 px-3 py-2 text-left bg-surface-1 border border-border rounded-lg shadow-[0_8px_24px_rgba(0,0,0,0.45)]"
					style={{
						width: Math.min(width, window.innerWidth - 2 * EDGE),
						left: place?.left ?? 0,
						top: place?.top ?? 0,
						// Placed on the second pass; until then it must not flash
						// at the top-left corner.
						visibility: place ? "visible" : "hidden",
					}}
				>
					{label && (
						<div className="num text-[9.5px] tracking-[0.06em] text-text-faint">
							{label.toUpperCase()}
						</div>
					)}
					<div className="text-[12.5px] leading-snug text-text-secondary break-words">
						{content}
					</div>
				</div>
			)}
		</>
	);
}
