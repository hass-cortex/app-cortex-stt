import { getEngineStatus, loadModel, setDefaultModel, unloadModel } from "@/api/engine";
import { POLL_INTERVALS, queryKeys } from "@/lib/constants";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

export function useEngineStatus() {
	return useQuery({
		queryKey: queryKeys.engine.status(),
		queryFn: getEngineStatus,
		refetchInterval: POLL_INTERVALS.ENGINE_STATUS,
	});
}

export function useSetDefaultModel() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (modelId: string) => setDefaultModel(modelId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.engine.all });
			queryClient.invalidateQueries({ queryKey: queryKeys.settings.all });
		},
	});
}

export function useLoadModel() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: ({ modelId, poolSize }: { modelId: string; poolSize?: number }) =>
			loadModel(modelId, poolSize),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.engine.all });
		},
	});
}

export function useUnloadModel() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (modelId: string) => unloadModel(modelId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.engine.all });
		},
	});
}
