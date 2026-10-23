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
import { useEngineLive } from "@/hooks/use-engine";
import { useInvalidatingMutation } from "@/hooks/use-invalidating-mutation";
import { queryKeys } from "@/lib/constants";

export function useModels() {
	// Live load-state updates (lazy load by STT requests, idle/LRU
	// offload) — `is_loaded` refreshes without polling.
	useEngineLive();

	return useQuery({
		queryKey: queryKeys.models.list(),
		queryFn: listModels,
		refetchInterval: (query) => {
			const data = query.state.data;
			return data?.some((m) =>
				m.variants.some((v) => v.status === "downloading" || v.status === "queued" || v.updating),
			)
				? 2000
				: false;
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

/** A Variant of a model: `quant` null for a custom model. */
export interface VariantRef {
	modelId: string;
	quant: string | null;
}

export function useCancelDownload() {
	return useInvalidatingMutation({
		mutationFn: ({ modelId, quant }: VariantRef) => cancelDownload(modelId, quant),
		invalidates: [queryKeys.models.all],
	});
}

/** Uninstall one Variant (or, with `quant: undefined`, the whole model). */
export function useDeleteModel() {
	return useInvalidatingMutation({
		mutationFn: ({ modelId, quant }: { modelId: string; quant?: string | null }) =>
			deleteModel(modelId, quant),
		invalidates: [queryKeys.models.all, queryKeys.engine.all],
	});
}

export function useScanCustomModels() {
	return useInvalidatingMutation({
		mutationFn: scanCustomModels,
		invalidates: [queryKeys.models.all],
	});
}

/** Subscribe to one Variant's download progress via SSE */
export function useDownloadProgress(
	modelId: string | null,
	quant: string | null,
): DownloadProgress | null {
	const [progress, setProgress] = useState<DownloadProgress | null>(null);
	const cleanupRef = useRef<(() => void) | null>(null);

	useEffect(() => {
		if (!modelId) {
			setProgress(null);
			return;
		}

		const query = quant ? `?quant=${encodeURIComponent(quant)}` : "";
		cleanupRef.current = subscribeSSE(
			`/api/models/${encodeURIComponent(modelId)}/download/progress${query}`,
			(data) => setProgress(data as DownloadProgress),
			() => setProgress(null),
		);

		return () => {
			cleanupRef.current?.();
			cleanupRef.current = null;
		};
	}, [modelId, quant]);

	return progress;
}
