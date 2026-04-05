import { LoginPage } from "@/pages/login";
import { useEffect, useState } from "react";

const API_KEY_STORAGE_KEY = "cortex-stt-api-key";

/** When accessed via HA ingress, auth is handled by HA — skip login. */
const isIngress = !!(window as unknown as { __INGRESS_PATH__?: string }).__INGRESS_PATH__;

export function AuthGate({ children }: { children: React.ReactNode }) {
	const [hasKey, setHasKey] = useState(() => {
		if (isIngress) return true;
		try {
			return !!localStorage.getItem(API_KEY_STORAGE_KEY);
		} catch {
			return false;
		}
	});

	// Listen for storage changes (login/logout from other tabs)
	useEffect(() => {
		const handler = () => {
			const key = localStorage.getItem(API_KEY_STORAGE_KEY);
			setHasKey(!!key);
		};
		window.addEventListener("storage", handler);
		return () => window.removeEventListener("storage", handler);
	}, []);

	if (!hasKey) {
		return <LoginPage onLogin={() => setHasKey(true)} />;
	}

	return <>{children}</>;
}
