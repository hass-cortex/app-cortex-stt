import { Plus } from "lucide-react";
import { useState } from "react";
import { GenerateKeyModal } from "@/components/keys/generate-key-modal";
import { KeyList } from "@/components/keys/key-list";
import { Button } from "@/components/ui/button";
import { Card, CardHeader } from "@/components/ui/card";

export function KeysPage() {
	const [showGenerate, setShowGenerate] = useState(false);

	return (
		<div className="flex flex-col gap-4">
			<div className="flex items-end justify-between gap-4 flex-wrap">
				<div>
					<h1 className="text-[21px] font-semibold tracking-[-0.01em] text-text-primary">
						API Keys
					</h1>
					<p className="text-[12.5px] text-text-secondary mt-1 max-w-3xl">
						Required for the HTTP API and WebSocket sessions. Requests arriving through Home
						Assistant ingress are already authenticated and need none.
					</p>
				</div>
				<Button icon={<Plus size={14} strokeWidth={1.8} />} onClick={() => setShowGenerate(true)}>
					Generate key
				</Button>
			</div>

			<KeyList onGenerate={() => setShowGenerate(true)} />

			<div className="flex flex-col xl:flex-row gap-4">
				<Card className="flex-1 min-w-0">
					<CardHeader title="Using a key" description="the same key works for both transports" />
					<pre className="num mt-3 px-3.5 py-3 bg-surface-0 border border-border rounded-lg text-[11.5px] leading-relaxed text-text-secondary overflow-x-auto">
						<code>{`curl -X POST http://<host>:8769/api/transcribe \\
  -H "Authorization: Bearer cx_…" \\
  -H "Content-Type: audio/wav" \\
  --data-binary @clip.wav \\
  -G --data-urlencode "model=<model-id>" \\
     --data-urlencode "language=zh-TW" \\
     --data-urlencode "capture_device=kitchen"`}</code>
					</pre>
					<p className="mt-3 text-[11.5px] leading-relaxed text-text-muted">
						Sending <span className="num text-text-secondary">capture_device</span> is what makes
						per-microphone quality visible later — the server cannot infer which microphone recorded
						a clip.
					</p>
				</Card>

				<Card className="xl:w-[360px] shrink-0">
					<CardHeader title="Scope" description="one level, deliberately" />
					<p className="mt-3 text-[12px] leading-relaxed text-text-secondary">
						A key grants the whole HTTP API — transcription, model management, history and settings.
						There are no read-only keys.
					</p>
					<p className="mt-2.5 text-[12px] leading-relaxed text-text-muted">
						Give each caller its own key, so revoking one does not silence the rest and “last used”
						means something.
					</p>
				</Card>
			</div>

			<GenerateKeyModal open={showGenerate} onClose={() => setShowGenerate(false)} />
		</div>
	);
}
