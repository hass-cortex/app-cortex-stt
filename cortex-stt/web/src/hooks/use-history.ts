import { deleteAllHistory, deleteHistoryRecord, getHistoryDetail, listHistory } from "@/api/history";
import { subscribeSSE } from "@/api/client";
import type { HistoryFilters } from "@/api/types";
import { queryKeys } from "@/lib/constants";
import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

export function useHistoryList(filters?: HistoryFilters) {
	const queryClient = useQueryClient();

	// Subscribe to SSE for live history updates instead of polling.
	useEffect(() => {
		const cleanup = subscribeSSE(
			"/api/history/live",
			() => {
				queryClient.invalidateQueries({ queryKey: queryKeys.history.all });
			},
			undefined, // onError
			"new_record", // named SSE event
		);
		return cleanup;
	}, [queryClient]);

	return useQuery({
		queryKey: queryKeys.history.list(filters as Record<string, string> | undefined),
		queryFn: () => listHistory(filters),
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

export function useDeleteAllHistory() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: deleteAllHistory,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.history.all });
		},
	});
}
