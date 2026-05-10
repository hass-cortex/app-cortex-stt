import { useQuery } from "@tanstack/react-query";
import { generateKey, listKeys, revokeKey } from "@/api/keys";
import { useInvalidatingMutation } from "@/hooks/use-invalidating-mutation";
import { queryKeys } from "@/lib/constants";

export function useKeys() {
	return useQuery({
		queryKey: queryKeys.keys.list(),
		queryFn: listKeys,
	});
}

export function useGenerateKey() {
	return useInvalidatingMutation({
		mutationFn: (name: string) => generateKey(name),
		invalidates: [queryKeys.keys.all],
	});
}

export function useRevokeKey() {
	return useInvalidatingMutation({
		mutationFn: (id: string) => revokeKey(id),
		invalidates: [queryKeys.keys.all],
	});
}
