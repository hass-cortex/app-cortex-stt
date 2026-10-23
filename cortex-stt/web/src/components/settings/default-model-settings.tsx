import { Save } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { useToast } from "@/components/ui/toast";
import { useSetDefaultModel } from "@/hooks/use-engine";
import { useModels, useSetPreferredQuant } from "@/hooks/use-models";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { variantKey } from "@/lib/variant";

export function DefaultModelSettings() {
	const { data: models } = useModels();
	const { data: settings } = useSettings();
	const setDefaultMutation = useSetDefaultModel();
	const setPreferredMutation = useSetPreferredQuant();
	const updateSettingsMutation = useUpdateSettings();
	const { toast } = useToast();
	const runSavePreload = useMutationToast(updateSettingsMutation, {
		success: "Pre-load setting saved",
	});

	// The default names a model; its Preferred variant picks the quant, so
	// choosing a Variant here sets both.
	const candidates = (models ?? []).flatMap((m) =>
		m.variants
			.filter((v) => v.status === "downloaded")
			.map((v) => ({
				key: variantKey(m.id, v.quant),
				modelId: m.id,
				quant: v.quant,
				label: v.quant ? `${m.name} (${v.quant})` : m.name,
			})),
	);
	const defaultModel = models?.find((m) => m.id === settings?.default_model);
	const defaultKey = defaultModel
		? variantKey(defaultModel.id, defaultModel.preferred_quant)
		: (settings?.default_model ?? "");
	const [picked, setPicked] = useState<string | null>(null);
	const selectedKey = picked ?? defaultKey;
	const [preloadModel, setPreloadModel] = useState(settings?.preload_default_model ?? false);

	const modelOptions = [
		{ value: "", label: "" },
		...candidates.map((c) => ({ value: c.key, label: c.label })),
	];

	const handleSetDefault = async () => {
		const candidate = candidates.find((c) => c.key === selectedKey);
		if (!candidate) return;
		try {
			if (candidate.modelId !== settings?.default_model)
				await setDefaultMutation.mutateAsync(candidate.modelId);
			if (candidate.quant)
				await setPreferredMutation.mutateAsync({
					modelId: candidate.modelId,
					quant: candidate.quant,
				});
			setPicked(null);
			toast(`${candidate.label} is now the default`, "success");
		} catch (err) {
			toast(`Failed: ${(err as Error).message}`, "error");
		}
	};

	const handleSavePreload = () => {
		runSavePreload({ preload_default_model: preloadModel });
	};

	const preloadChanged = preloadModel !== (settings?.preload_default_model ?? false);

	return (
		<Card>
			<CardHeader
				title="Default Model"
				description="Model and quant used when a request names no model"
			/>
			<div className="space-y-4">
				<div className="flex items-center gap-2">
					<Select
						options={modelOptions}
						value={selectedKey}
						onChange={(e) => setPicked(e.target.value)}
						placeholder="Select model"
						className="flex-1"
					/>
					<Button
						onClick={handleSetDefault}
						loading={setDefaultMutation.isPending || setPreferredMutation.isPending}
						disabled={!selectedKey || selectedKey === defaultKey}
					>
						Apply
					</Button>
				</div>
				{defaultKey && selectedKey === defaultKey && (
					<Badge variant="success">Current default</Badge>
				)}

				<hr className="border-border" />

				<div className="space-y-2">
					<label className="flex items-center gap-2 cursor-pointer">
						<input
							type="checkbox"
							checked={preloadModel}
							onChange={(e) => setPreloadModel(e.target.checked)}
							className="rounded border-border"
						/>
						<span className="text-sm text-text-primary">Pre-load on startup</span>
					</label>
					<p className="text-xs text-text-muted">
						{preloadModel
							? "Default model is loaded into memory when the server starts, eliminating first-request latency."
							: "Models are loaded on-demand when the first transcription request arrives."}
					</p>
					{preloadChanged && (
						<Button
							size="sm"
							icon={<Save size={14} />}
							onClick={handleSavePreload}
							loading={updateSettingsMutation.isPending}
						>
							Save
						</Button>
					)}
				</div>
			</div>
		</Card>
	);
}
