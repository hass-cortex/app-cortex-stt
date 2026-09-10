import { X } from "lucide-react";
import { type ReactNode, type RefObject, useEffect, useRef } from "react";

/** How much horizontal room the body needs. `md` is a form or a
 *  question; `xl` is for a body that puts two lists side by side. */
type ModalWidth = "md" | "lg" | "xl";

const widthClass: Record<ModalWidth, string> = {
	md: "max-w-md",
	lg: "max-w-2xl",
	xl: "max-w-4xl",
};

interface ModalProps {
	open: boolean;
	onClose: () => void;
	title: string;
	children: ReactNode;
	footer?: ReactNode;
	width?: ModalWidth;
	/** Control to focus on open. `showModal()` focuses the first focusable
	 *  descendant — the close button — and runs after React's own
	 *  `autoFocus`, so the choice has to be made here. */
	initialFocus?: RefObject<HTMLElement | null>;
}

export function Modal({
	open,
	onClose,
	title,
	children,
	footer,
	width = "md",
	initialFocus,
}: ModalProps) {
	const dialogRef = useRef<HTMLDialogElement>(null);

	useEffect(() => {
		const dialog = dialogRef.current;
		if (!dialog) return;

		if (open) {
			dialog.showModal();
			initialFocus?.current?.focus();
		} else {
			dialog.close();
		}
	}, [open, initialFocus]);

	useEffect(() => {
		const dialog = dialogRef.current;
		if (!dialog) return;

		const handleCancel = (e: Event) => {
			e.preventDefault();
			onClose();
		};
		dialog.addEventListener("cancel", handleCancel);
		return () => dialog.removeEventListener("cancel", handleCancel);
	}, [onClose]);

	if (!open) return null;

	return (
		<dialog
			ref={dialogRef}
			className="fixed inset-0 z-50 flex items-center justify-center bg-transparent backdrop:bg-black/60 p-0 m-0 w-full h-full max-w-none max-h-none"
			onClick={(e) => {
				if (e.target === dialogRef.current) onClose();
			}}
			onKeyDown={(e) => {
				if (e.key === "Escape") onClose();
			}}
		>
			{/* The body scrolls, the title and the footer do not: a long body
			    used to grow the dialog past the viewport, which put Start
			    somewhere the page could not be scrolled to. */}
			<div
				className={`bg-surface-2 border border-border rounded-[10px] shadow-2xl ${widthClass[width]} w-full max-h-[90vh] flex flex-col px-5 py-4`}
			>
				<div className="flex items-center justify-between mb-4 shrink-0">
					<h2 className="text-[15px] font-semibold text-text-primary">{title}</h2>
					<button
						type="button"
						onClick={onClose}
						className="p-1 rounded-md text-text-muted hover:text-text-primary hover:bg-surface-3 transition-colors cursor-pointer"
					>
						<X size={18} />
					</button>
				</div>
				<div className="min-h-0 flex-1 overflow-y-auto text-[12.5px] text-text-secondary">
					{children}
				</div>
				{footer && (
					<div className="flex items-center justify-end gap-2 mt-5 pt-3.5 border-t border-border shrink-0">
						{footer}
					</div>
				)}
			</div>
		</dialog>
	);
}
