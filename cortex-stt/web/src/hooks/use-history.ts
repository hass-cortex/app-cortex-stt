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

// Every history mutation (create / delete / drop-audio) changes the
// aggregate numbers shown on the Dashboard (Audio MB, Storage usage,
// today/total duration), so invalidate those queries alongside the
// history list itself. Without this the Dashboard reads stale cached
// values until staleTime elapses or the page is reloaded.
const HISTORY_MUTATION_INVALIDATES = [
	queryKeys.history.all,
	queryKeys.system.metrics(),
	queryKeys.system.storage(),
];

export function useHistoryList(filters?: HistoryFilters) {
	const queryClient = useQueryClient();

	// Subscribe to SSE for live history updates instead of polling.
	useEffect(() => {
		const cleanup = subscribeSSE(
			"/api/history/live",
			() => {
				for (const key of HISTORY_MUTATION_INVALIDATES) {
					queryClient.invalidateQueries({ queryKey: key });
				}
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
		invalidates: HISTORY_MUTATION_INVALIDATES,
	});
}

export function useDeleteAllHistory() {
	return useInvalidatingMutation({
		mutationFn: deleteAllHistory,
		invalidates: HISTORY_MUTATION_INVALIDATES,
	});
}
