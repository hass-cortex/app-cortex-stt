import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { subscribeSSE } from "@/api/client";
import {
	deleteAllHistory,
	deleteHistoryRecord,
	getHistoryDetail,
	listHistory,
} from "@/api/history";
import type { HistoryFilters } from "@/api/types";
import { useInvalidatingMutation } from "@/hooks/use-invalidating-mutation";
import { queryKeys } from "@/lib/constants";

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
	return useInvalidatingMutation({
		mutationFn: (id: string) => deleteHistoryRecord(id),
		invalidates: [queryKeys.history.all],
	});
}

export function useDeleteAllHistory() {
	return useInvalidatingMutation({
		mutationFn: deleteAllHistory,
		invalidates: [queryKeys.history.all],
	});
}
