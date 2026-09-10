import { useQuery } from "@tanstack/react-query";
import { type DecodedClip, decodeClip } from "@/lib/audio";

/**
 * Fetch and decode a stored clip so its waveform can be drawn. Only ever
 * called for a record the reader has opened — decoding every row would
 * download the whole audio directory.
 */
export function useClipPeaks(url: string | null) {
	return useQuery<DecodedClip>({
		queryKey: ["clip", url],
		queryFn: async () => {
			const response = await fetch(url ?? "");
			if (!response.ok) throw new Error(`HTTP ${response.status}`);
			return decodeClip(await response.arrayBuffer());
		},
		enabled: !!url,
		staleTime: Number.POSITIVE_INFINITY,
		retry: false,
	});
}
