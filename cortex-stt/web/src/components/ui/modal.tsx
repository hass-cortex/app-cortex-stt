import { X } from "lucide-react";
import { type ReactNode, useEffect, useRef } from "react";

interface ModalProps {
	open: boolean;
	onClose: () => void;
	title: string;
	children: ReactNode;
	footer?: ReactNode;
}

export function Modal({ open, onClose, title, children, footer }: ModalProps) {
	const dialogRef = useRef<HTMLDialogElement>(null);

	useEffect(() => {
		const dialog = dialogRef.current;
		if (!dialog) return;

		if (open) {
			dialog.showModal();
		} else {
			dialog.close();
		}
	}, [open]);

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
			<div className="bg-surface-2 border border-border rounded-xl shadow-2xl max-w-md w-full p-5">
				<div className="flex items-center justify-between mb-4">
					<h2 className="text-lg font-semibold text-text-primary">{title}</h2>
					<button
						type="button"
						onClick={onClose}
						className="p-1 rounded-md text-text-muted hover:text-text-primary hover:bg-surface-3 transition-colors cursor-pointer"
					>
						<X size={18} />
					</button>
				</div>
				<div className="text-sm text-text-secondary">{children}</div>
				{footer && (
					<div className="flex items-center justify-end gap-2 mt-5 pt-4 border-t border-border">
						{footer}
					</div>
				)}
			</div>
		</dialog>
	);
}
