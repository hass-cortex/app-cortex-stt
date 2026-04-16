import { getEngineStatus, loadModel, setDefaultModel, unloadModel } from "@/api/engine";
import { useInvalidatingMutation } from "@/hooks/use-invalidating-mutation";
import { POLL_INTERVALS, queryKeys } from "@/lib/constants";
import { useQuery } from "@tanstack/react-query";

export function useEngineStatus() {
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
