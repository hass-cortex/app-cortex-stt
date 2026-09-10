import { AlertTriangle } from "lucide-react";
import {
	createContext,
	type ReactNode,
	useCallback,
	useContext,
	useMemo,
	useRef,
	useState,
} from "react";
import { Button } from "./button";
import { Modal } from "./modal";

export interface ConfirmOptions {
	title: string;
	/** What the reader needs to weigh before answering. */
	body?: ReactNode;
	confirmLabel?: string;
	cancelLabel?: string;
	/** Destructive actions get the danger button; everything else is primary. */
	destructive?: boolean;
}

type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>;

const ConfirmContext = createContext<ConfirmFn | null>(null);

/**
 * An in-product confirmation, so a destructive step keeps the app's own
 * chrome instead of handing the reader a browser dialog that names the
 * host and cannot say what the action costs.
 */
export function ConfirmProvider({ children }: { children: ReactNode }) {
	const [options, setOptions] = useState<ConfirmOptions | null>(null);
	const resolveRef = useRef<((answer: boolean) => void) | null>(null);
	const cancelRef = useRef<HTMLButtonElement>(null);

	const confirm = useCallback<ConfirmFn>((next) => {
		setOptions(next);
		return new Promise<boolean>((resolve) => {
			resolveRef.current = resolve;
		});
	}, []);

	const settle = useCallback((answer: boolean) => {
		resolveRef.current?.(answer);
		resolveRef.current = null;
		setOptions(null);
	}, []);

	const value = useMemo(() => confirm, [confirm]);

	return (
		<ConfirmContext.Provider value={value}>
			{children}
			<Modal
				open={options !== null}
				onClose={() => settle(false)}
				title={options?.title ?? ""}
				initialFocus={cancelRef}
				footer={
					<>
						{/* On a destructive prompt the safe answer is the one Enter
						    lands on. */}
						<Button ref={cancelRef} variant="ghost" onClick={() => settle(false)}>
							{options?.cancelLabel ?? "Cancel"}
						</Button>
						<Button
							variant={options?.destructive ? "danger" : "primary"}
							onClick={() => settle(true)}
						>
							{options?.confirmLabel ?? "Confirm"}
						</Button>
					</>
				}
			>
				<div className="flex gap-3">
					{options?.destructive && (
						<AlertTriangle size={16} strokeWidth={1.8} className="text-error shrink-0 mt-px" />
					)}
					<div className="text-[12.5px] leading-relaxed text-text-secondary">{options?.body}</div>
				</div>
			</Modal>
		</ConfirmContext.Provider>
	);
}

export function useConfirm(): ConfirmFn {
	const ctx = useContext(ConfirmContext);
	if (!ctx) throw new Error("useConfirm must be used within ConfirmProvider");
	return ctx;
}
