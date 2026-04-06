import { generateKey, listKeys, revokeKey } from "@/api/keys";
import { queryKeys } from "@/lib/constants";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

export function useKeys() {
	return useQuery({
		queryKey: queryKeys.keys.list(),
		queryFn: listKeys,
	});
}

export function useGenerateKey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (name: string) => generateKey(name),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.keys.all });
		},
	});
}

export function useRevokeKey() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (id: string) => revokeKey(id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.keys.all });
		},
	});
}
