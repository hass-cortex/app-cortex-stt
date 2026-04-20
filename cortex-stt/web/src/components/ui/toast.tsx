import { AlertCircle, CheckCircle, Info, X, XCircle } from "lucide-react";
import { createContext, type ReactNode, useCallback, useContext, useMemo, useState } from "react";

type ToastVariant = "success" | "error" | "warning" | "info";

interface Toast {
	id: string;
	message: string;
	variant: ToastVariant;
}

interface ToastContextValue {
	toast: (message: string, variant?: ToastVariant) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

const icons: Record<ToastVariant, ReactNode> = {
	success: <CheckCircle size={16} className="text-success" />,
	error: <XCircle size={16} className="text-error" />,
	warning: <AlertCircle size={16} className="text-warning" />,
	info: <Info size={16} className="text-info" />,
};

const borderColors: Record<ToastVariant, string> = {
	success: "border-l-success",
	error: "border-l-error",
	warning: "border-l-warning",
	info: "border-l-info",
};

export function ToastProvider({ children }: { children: ReactNode }) {
	const [toasts, setToasts] = useState<Toast[]>([]);

	const toast = useCallback((message: string, variant: ToastVariant = "info") => {
		const id = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
		setToasts((prev) => [...prev, { id, message, variant }]);
		setTimeout(() => {
			setToasts((prev) => prev.filter((t) => t.id !== id));
		}, 4000);
	}, []);

	const dismiss = useCallback((id: string) => {
		setToasts((prev) => prev.filter((t) => t.id !== id));
	}, []);

	const value = useMemo(() => ({ toast }), [toast]);

	return (
		<ToastContext.Provider value={value}>
			{children}
			<div className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-sm" aria-live="polite">
				{toasts.map((t) => (
					<div
						key={t.id}
						className={`flex items-center gap-2.5 bg-surface-2 border border-border border-l-4 ${borderColors[t.variant]} rounded-lg px-3.5 py-2.5 shadow-lg`}
					>
						{icons[t.variant]}
						<p className="flex-1 text-sm text-text-primary">{t.message}</p>
						<button
							type="button"
							onClick={() => dismiss(t.id)}
							className="p-0.5 text-text-muted hover:text-text-primary transition-colors cursor-pointer"
						>
							<X size={14} />
						</button>
					</div>
				))}
			</div>
		</ToastContext.Provider>
	);
}

export function useToast(): ToastContextValue {
	const ctx = useContext(ToastContext);
	if (!ctx) throw new Error("useToast must be used within ToastProvider");
	return ctx;
}
