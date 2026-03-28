import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { useToast } from "@/components/ui/toast";
import { useSettings, useUpdateSettings } from "@/hooks/use-settings";
import { Plus, Save, X } from "lucide-react";
import { useState } from "react";

export function CorsSettings() {
	const { data: settings } = useSettings();
	const updateMutation = useUpdateSettings();
	const { toast } = useToast();

	const [origins, setOrigins] = useState<string[]>(settings?.cors_origins ?? []);
	const [newOrigin, setNewOrigin] = useState("");

	const addOrigin = () => {
		const trimmed = newOrigin.trim();
		if (!trimmed || origins.includes(trimmed)) return;
		setOrigins([...origins, trimmed]);
		setNewOrigin("");
	};

	const removeOrigin = (origin: string) => {
		setOrigins(origins.filter((o) => o !== origin));
	};

	const handleSave = () => {
		updateMutation.mutate(
			{ cors_origins: origins },
			{
				onSuccess: () => toast("CORS settings saved", "success"),
				onError: (err) => toast(`Failed: ${err.message}`, "error"),
			},
		);
	};

	return (
		<Card>
			<CardHeader
				title="CORS"
				description="Configure allowed origins for cross-origin HTTP API requests"
			/>
			<div className="space-y-4">
				<div className="flex items-end gap-2">
					<Input
						label="Add origin"
						placeholder="https://example.com"
						value={newOrigin}
						onChange={(e) => setNewOrigin(e.target.value)}
						onKeyDown={(e) => {
							if (e.key === "Enter") addOrigin();
						}}
					/>
					<Button
						variant="secondary"
						icon={<Plus size={14} />}
						onClick={addOrigin}
						disabled={!newOrigin.trim()}
					>
						Add
					</Button>
				</div>

				{origins.length > 0 && (
					<div className="space-y-1">
						{origins.map((origin) => (
							<div
								key={origin}
								className="flex items-center justify-between px-3 py-1.5 bg-surface-3 rounded-lg"
							>
								<code className="text-xs text-text-primary">{origin}</code>
								<button
									type="button"
									onClick={() => removeOrigin(origin)}
									className="p-0.5 text-text-muted hover:text-error transition-colors cursor-pointer"
								>
									<X size={14} />
								</button>
							</div>
						))}
					</div>
				)}

				{origins.length === 0 && (
					<p className="text-xs text-text-muted">
						No origins configured. All cross-origin requests will be blocked.
					</p>
				)}

				<Button icon={<Save size={14} />} onClick={handleSave} loading={updateMutation.isPending}>
					Save CORS
				</Button>
			</div>
		</Card>
	);
}
