import { GenerateKeyModal } from "@/components/keys/generate-key-modal";
import { KeyList } from "@/components/keys/key-list";
import { useState } from "react";

export function KeysPage() {
	const [showGenerate, setShowGenerate] = useState(false);

	return (
		<div className="space-y-6">
			<div>
				<h1 className="text-xl font-bold text-text-primary">API Keys</h1>
				<p className="text-sm text-text-secondary mt-1">
					Generate and manage API keys for HTTP API authentication
				</p>
			</div>

			<KeyList onGenerate={() => setShowGenerate(true)} />
			<GenerateKeyModal open={showGenerate} onClose={() => setShowGenerate(false)} />
		</div>
	);
}
