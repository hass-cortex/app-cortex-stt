import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { subscribeSSE } from "@/api/client";
import {
	cancelDownload,
	deleteModel,
	downloadModel,
	listModels,
	scanCustomModels,
} from "@/api/models";
import type { DownloadProgress } from "@/api/types";
import { useInvalidatingMutation } from "@/hooks/use-invalidating-mutation";
import { queryKeys } from "@/lib/constants";

export function useModels() {
	return useQuery({
		queryKey: queryKeys.models.list(),
		queryFn: listModels,
		refetchInterval: (query) => {
			const data = query.state.data;
			return data?.some((m) => m.status === "downloading" || m.status === "queued") ? 2000 : false;
		},
	});
}

export function useDownloadModel() {
	return useInvalidatingMutation({
		mutationFn: ({ modelId, quant }: { modelId: string; quant?: string }) =>
			downloadModel(modelId, quant),
		invalidates: [queryKeys.models.all],
	});
}

export function useCancelDownload() {
	return useInvalidatingMutation({
		mutationFn: (modelId: string) => cancelDownload(modelId),
		invalidates: [queryKeys.models.all],
	});
}

export function useDeleteModel() {
	return useInvalidatingMutation({
		mutationFn: (modelId: string) => deleteModel(modelId),
		invalidates: [queryKeys.models.all, queryKeys.engine.all],
	});
}

export function useScanCustomModels() {
	return useInvalidatingMutation({
		mutationFn: scanCustomModels,
		invalidates: [queryKeys.models.all],
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
