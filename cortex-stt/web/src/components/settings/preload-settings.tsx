import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { useModels } from "@/hooks/use-models";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { variantKey } from "@/lib/variant";

export function PreloadSettings() {
	const { data: models } = useModels();
	const { data: settings } = useSettings();
	const updateSettingsMutation = useUpdateSettings();
	const runSave = useMutationToast(updateSettingsMutation, { success: "Pre-load saved" });

	const saved = settings?.preload ?? "";
	const [picked, setPicked] = useState<string | null>(null);
	const selected = picked ?? saved;

	const options = [
		{ value: "", label: "None" },
		...(models ?? []).flatMap((m) =>
			m.variants
				.filter((v) => v.status === "downloaded")
				.map((v) => ({
					value: variantKey(m.id, v.quant),
					label: v.quant ? `${m.name} (${v.quant})` : m.name,
				})),
		),
	];

	return (
		<Card>
			<CardHeader
				title="Pre-load on startup"
				description="Loaded when the service starts, so its first request does not wait for it. Every other Variant loads on its first request."
			/>
			<div className="flex items-center gap-2">
				<Select
					options={options}
					value={selected}
					onChange={(e) => setPicked(e.target.value)}
					aria-label="Variant to pre-load"
					className="flex-1"
				/>
				<Button
					onClick={() => runSave({ preload: selected || null })}
					loading={updateSettingsMutation.isPending}
					disabled={selected === saved}
				>
					Save
				</Button>
			</div>
		</Card>
	);
}
