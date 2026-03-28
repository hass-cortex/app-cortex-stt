import { getSettings, updateSettings } from "@/api/settings";
import type { AppSettings } from "@/api/types";
import { queryKeys } from "@/lib/constants";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

export function useSettings() {
	return useQuery({
		queryKey: queryKeys.settings.all,
		queryFn: getSettings,
	});
}

export function useUpdateSettings() {
	const queryClient = useQueryClient();
	return useMutation({
		mutationFn: (settings: Partial<AppSettings>) => updateSettings(settings),
		onSuccess: (data) => {
			queryClient.setQueryData(queryKeys.settings.all, data);
		},
	});
}
