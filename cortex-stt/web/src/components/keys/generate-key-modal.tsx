import { Check, Copy } from "lucide-react";
import { useState } from "react";
import type { GeneratedKey } from "@/api/types";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Modal } from "@/components/ui/modal";
import { useToast } from "@/components/ui/toast";
import { useGenerateKey } from "@/hooks/use-keys";
import { copyToClipboard } from "@/lib/clipboard";

interface GenerateKeyModalProps {
	open: boolean;
	onClose: () => void;
}

export function GenerateKeyModal({ open, onClose }: GenerateKeyModalProps) {
	const [name, setName] = useState("");
	const [generatedKey, setGeneratedKey] = useState<GeneratedKey | null>(null);
	const [copied, setCopied] = useState(false);
	const generateMutation = useGenerateKey();
	const { toast } = useToast();

	const handleGenerate = () => {
		if (!name.trim()) return;
		generateMutation.mutate(name.trim(), {
			onSuccess: (data) => {
				setGeneratedKey(data);
			},
			onError: (err) => toast(`Failed: ${err.message}`, "error"),
		});
	};

	const handleCopy = async () => {
		if (!generatedKey) return;
		try {
			await copyToClipboard(generatedKey.key);
			setCopied(true);
			setTimeout(() => setCopied(false), 2000);
		} catch {
			toast("Failed to copy to clipboard", "error");
		}
	};

	const handleClose = () => {
		setName("");
		setGeneratedKey(null);
		setCopied(false);
		onClose();
	};

	return (
		<Modal
			open={open}
			onClose={handleClose}
			title={generatedKey ? "API Key Created" : "Generate API Key"}
			footer={
				generatedKey ? (
					<Button onClick={handleClose}>Done</Button>
				) : (
					<>
						<Button variant="secondary" onClick={handleClose}>
							Cancel
						</Button>
						<Button
							onClick={handleGenerate}
							loading={generateMutation.isPending}
							disabled={!name.trim()}
						>
							Generate
						</Button>
					</>
				)
			}
		>
			{generatedKey ? (
				<div className="space-y-4">
					<div className="p-3 bg-warning/10 border border-warning/30 rounded-lg">
						<p className="text-xs font-medium text-warning">
							Copy this key now. It will not be shown again.
						</p>
					</div>

					<div className="flex items-center gap-2">
						<code className="flex-1 p-2.5 bg-surface-3 rounded-lg text-xs font-mono text-text-primary break-all select-all">
							{generatedKey.key}
						</code>
						<Button
							variant="secondary"
							size="sm"
							icon={copied ? <Check size={14} /> : <Copy size={14} />}
							onClick={handleCopy}
						>
							{copied ? "Copied" : "Copy"}
						</Button>
					</div>

					<p className="text-xs text-text-muted">
						Name: <span className="text-text-secondary">{generatedKey.name}</span>
					</p>
				</div>
			) : (
				<div className="space-y-3">
					<p>Give your API key a descriptive name to identify its purpose.</p>
					<Input
						label="Key name"
						placeholder="e.g., My Application"
						value={name}
						onChange={(e) => setName(e.target.value)}
						autoFocus
						onKeyDown={(e) => {
							if (e.key === "Enter") handleGenerate();
						}}
					/>
				</div>
			)}
		</Modal>
	);
}
