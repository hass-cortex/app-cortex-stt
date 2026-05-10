import type { UseMutationResult } from "@tanstack/react-query";
import { useToast } from "@/components/ui/toast";

type Message<TData> = string | ((data: TData) => string);

/**
 * Wrap a mutation with success/error toast feedback. Returns a function that
 * calls `mutation.mutate(vars)` with standardized toast callbacks.
 *
 * `messages.success` is shown on success (optionally as a function of the
 * response data). `messages.error` prefixes the error message (default
 * "Failed"), so the user sees `"<prefix>: <server message>"`.
 */
export function useMutationToast<TData, TVars>(
	mutation: UseMutationResult<TData, Error, TVars>,
	messages: { success: Message<TData>; error?: string },
) {
	const { toast } = useToast();
	return (vars: TVars) =>
		mutation.mutate(vars, {
			onSuccess: (data) =>
				toast(
					typeof messages.success === "function" ? messages.success(data) : messages.success,
					"success",
				),
			onError: (err) => toast(`${messages.error ?? "Failed"}: ${err.message}`, "error"),
		});
}
