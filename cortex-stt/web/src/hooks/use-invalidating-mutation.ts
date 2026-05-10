import { useMutation, useQueryClient } from "@tanstack/react-query";

/**
 * A thin wrapper around [`useMutation`] that invalidates the given query keys
 * on success. Use this for mutations whose only side effect is "re-fetch X".
 * For mutations that also need to write cache directly (e.g. `setQueryData`),
 * fall back to `useMutation` with a custom `onSuccess`.
 */
export function useInvalidatingMutation<TData, TVars>(opts: {
	mutationFn: (vars: TVars) => Promise<TData>;
	invalidates: readonly (readonly unknown[])[];
}) {
	const qc = useQueryClient();
	return useMutation({
		mutationFn: opts.mutationFn,
		onSuccess: () => {
			for (const key of opts.invalidates) {
				qc.invalidateQueries({ queryKey: key });
			}
		},
	});
}
