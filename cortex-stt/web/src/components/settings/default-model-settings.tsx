import { Save } from "lucide-react";
import { useState } from "react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Select } from "@/components/ui/select";
import { useSetDefaultModel } from "@/hooks/use-engine";
import { useModels } from "@/hooks/use-models";
import { useMutationToast } from "@/hooks/use-mutation-toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";

export function DefaultModelSettings() {
	const { data: models } = useModels();
	const { data: settings } = useSettings();
	const setDefaultMutation = useSetDefaultModel();
	const updateSettingsMutation = useUpdateSettings();
	const runSetDefault = useMutationToast(setDefaultMutation, { success: "Default model updated" });
	const runSavePreload = useMutationToast(updateSettingsMutation, {
		success: "Pre-load setting saved",
	});

	const defaultModel = settings?.default_model ?? "";
	const [selectedDefault, setSelectedDefault] = useState(defaultModel);
	const [preloadModel, setPreloadModel] = useState(settings?.preload_default_model ?? false);

	const downloadedModels = (models ?? []).filter(
		(m) =>
			m.status === "downloaded" ||
			m.status === "loaded" ||
			m.status === "loading" ||
			m.status === "custom",
	);

	const modelOptions = [
		{ value: "", label: "" },
		...downloadedModels.map((m) => ({ value: m.id, label: m.name })),
	];

	const handleSetDefault = () => {
		runSetDefault(selectedDefault);
	};

	const handleSavePreload = () => {
		runSavePreload({ preload_default_model: preloadModel });
	};

	const preloadChanged = preloadModel !== (settings?.preload_default_model ?? false);

	return (
		<Card>
			<CardHeader
				title="Default Model"
				description="Model used when no explicit model is specified"
			/>
			<div className="space-y-4">
				<div className="flex items-center gap-2">
					<Select
						options={modelOptions}
						value={selectedDefault}
						onChange={(e) => setSelectedDefault(e.target.value)}
						placeholder="Select model"
						className="flex-1"
					/>
					<Button
						onClick={handleSetDefault}
						loading={setDefaultMutation.isPending}
						disabled={selectedDefault === defaultModel}
					>
						Apply
					</Button>
				</div>
				{defaultModel && selectedDefault === defaultModel && (
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
