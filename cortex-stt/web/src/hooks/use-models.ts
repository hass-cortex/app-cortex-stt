import { subscribeSSE } from "@/api/client";
import {
	cancelDownload,
	deleteModel,
	downloadModel,
	listModels,
	scanCustomModels,
} from "@/api/models";
import type { DownloadProgress } from "@/api/types";
import { queryKeys } from "@/lib/constants";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

export function useModels() {
	return useQuery({
		queryKey: queryKeys.models.list(),
		queryFn: listModels,
		refetchInterval: (query) => {
			const data = query.state.data;
			return data?.some((m) => m.status === "downloading" || m.status === "queued")
				? 2000
				: false;
		},
	});
}

export function useDownloadModel() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (modelId: string) => downloadModel(modelId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.models.all });
		},
	});
}

export function useCancelDownload() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (modelId: string) => cancelDownload(modelId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.models.all });
		},
	});
}

export function useDeleteModel() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (modelId: string) => deleteModel(modelId),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.models.all });
			queryClient.invalidateQueries({ queryKey: queryKeys.engine.all });
		},
	});
}

export function useScanCustomModels() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: scanCustomModels,
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: queryKeys.models.all });
		},
	});
}

/** Subscribe to download progress for a specific model via SSE */
export function useDownloadProgress(modelId: string | null): DownloadProgress | null {
	const [progress, setProgress] = useState<DownloadProgress | null>(null);
	const cleanupRef = useRef<(() => void) | null>(null);

	useEffect(() => {
		if (!modelId) {
			setProgress(null);
			return;
		}

		cleanupRef.current = subscribeSSE(
			`/api/models/${encodeURIComponent(modelId)}/download/progress`,
			(data) => setProgress(data as DownloadProgress),
			() => setProgress(null),
		);

		return () => {
			cleanupRef.current?.();
			cleanupRef.current = null;
		};
	}, [modelId]);

	return progress;
}
