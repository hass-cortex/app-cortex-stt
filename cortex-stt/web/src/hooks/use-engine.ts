import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { subscribeSSE } from "@/api/client";
import { getEngineStatus, loadModel, setDefaultModel, unloadModel } from "@/api/engine";
import { useInvalidatingMutation } from "@/hooks/use-invalidating-mutation";
import { POLL_INTERVALS, queryKeys } from "@/lib/constants";

/** Subscribe to engine load-state changes (`/api/engine/live` SSE) and
 *  invalidate the engine + models queries on each event — lazy loads
 *  triggered by STT requests and idle/LRU offloads appear live. */
export function useEngineLive() {
	const queryClient = useQueryClient();

	useEffect(() => {
		const cleanup = subscribeSSE(
			"/api/engine/live",
			() => {
				queryClient.invalidateQueries({ queryKey: queryKeys.engine.all });
				queryClient.invalidateQueries({ queryKey: queryKeys.models.all });
			},
			undefined, // onError
			"engine_changed", // named SSE event
		);
		return cleanup;
	}, [queryClient]);
}

export function useEngineStatus() {
	// SSE push keeps this live; the interval below stays as a fallback
	// for missed events (e.g. SSE reconnect windows).
	useEngineLive();

	return useQuery({
		queryKey: queryKeys.engine.status(),
		queryFn: getEngineStatus,
		refetchInterval: POLL_INTERVALS.ENGINE_STATUS,
	});
}

export function useSetDefaultModel() {
	return useInvalidatingMutation({
		mutationFn: (modelId: string) => setDefaultModel(modelId),
		invalidates: [queryKeys.engine.all, queryKeys.settings.all],
	});
}

export function useLoadModel() {
	return useInvalidatingMutation({
		mutationFn: ({ modelId, poolSize }: { modelId: string; poolSize?: number }) =>
			loadModel(modelId, poolSize),
		invalidates: [queryKeys.engine.all, queryKeys.models.all],
	});
}

export function useUnloadModel() {
	return useInvalidatingMutation({
		mutationFn: (modelId: string) => unloadModel(modelId),
		invalidates: [queryKeys.engine.all, queryKeys.models.all],
	});
}
