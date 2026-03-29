import { cleanupHistory, deleteHistoryRecord, getHistoryDetail, listHistory } from "@/api/history";
import type { HistoryFilters } from "@/api/types";
import { queryKeys } from "@/lib/constants";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

export function useHistoryList(filters?: HistoryFilters) {
	return useQuery({
		queryKey: queryKeys.history.list(filters as Record<string, string> | undefined),
		queryFn: () => listHistory(filters),
		refetchInterval: 5000,
	});
}

export function useHistoryDetail(id: string | null) {
	return useQuery({
		queryKey: queryKeys.history.detail(id ?? ""),
		queryFn: () => getHistoryDetail(id ?? ""),
		enabled: !!id,
	});
}

export function useDeleteHistoryRecord() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (id: string) => deleteHistoryRecord(id),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.history.all });
		},
	});
}

export function useCleanupHistory() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: cleanupHistory,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.history.all });
		},
	});
}
