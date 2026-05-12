import { useEffect, useState } from "react";
import { LoginPage } from "@/pages/login";

const API_KEY_STORAGE_KEY = "cortex-stt-api-key";

/** When accessed via HA ingress, auth is handled by HA — skip the gate entirely. */
const isIngress = !!(window as unknown as { __INGRESS_PATH__?: string }).__INGRESS_PATH__;

export function AuthGate({ children }: { children: React.ReactNode }) {
	if (isIngress) return <>{children}</>;
	return <ApiKeyGate>{children}</ApiKeyGate>;
}

function ApiKeyGate({ children }: { children: React.ReactNode }) {
	const [hasKey, setHasKey] = useState(() => {
		try {
			return !!localStorage.getItem(API_KEY_STORAGE_KEY);
		} catch {
			return false;
		}
	});

	useEffect(() => {
		const handler = (e: StorageEvent) => {
			if (e.key !== null && e.key !== API_KEY_STORAGE_KEY) return;
			setHasKey(!!localStorage.getItem(API_KEY_STORAGE_KEY));
		};
		window.addEventListener("storage", handler);
		return () => window.removeEventListener("storage", handler);
	}, []);

	if (!hasKey) {
		return <LoginPage onLogin={() => setHasKey(true)} />;
	}

	return <>{children}</>;
}
