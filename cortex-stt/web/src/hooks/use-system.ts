import { useQuery } from "@tanstack/react-query";
import { getHealth, getMetrics, getStorageInfo, getSystemInfo } from "@/api/system";
import { POLL_INTERVALS, queryKeys } from "@/lib/constants";

export function useHealth() {
	return useQuery({
		queryKey: queryKeys.health,
		queryFn: getHealth,
		refetchInterval: POLL_INTERVALS.HEALTH,
	});
}

export function useSystemInfo() {
	return useQuery({
		queryKey: queryKeys.system.hardware(),
		queryFn: getSystemInfo,
		staleTime: 60_000, // Hardware info rarely changes
	});
}

export function useMetrics() {
	return useQuery({
		queryKey: queryKeys.system.metrics(),
		queryFn: getMetrics,
		refetchInterval: POLL_INTERVALS.METRICS,
	});
}

export function useStorageInfo() {
	return useQuery({
		queryKey: queryKeys.system.storage(),
		queryFn: getStorageInfo,
		staleTime: 30_000,
	});
}
