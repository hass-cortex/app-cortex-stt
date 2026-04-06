import {
	createContext,
	createElement,
	useCallback,
	useContext,
	useEffect,
	useMemo,
	useState,
} from "react";
import { THEME_KEY } from "./constants";

export type ThemeMode = "light" | "dark" | "auto";

interface ThemeContextValue {
	mode: ThemeMode;
	resolved: "light" | "dark";
	setMode: (mode: ThemeMode) => void;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

function getStoredTheme(): ThemeMode {
	try {
		const stored = localStorage.getItem(THEME_KEY);
		if (stored === "light" || stored === "dark" || stored === "auto") {
			return stored;
		}
	} catch {
		// localStorage unavailable (e.g., HA ingress iframe)
	}
	return "auto";
}

function getSystemPreference(): "light" | "dark" {
	if (typeof window === "undefined") return "dark";
	return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function applyTheme(resolved: "light" | "dark") {
	const root = document.documentElement;
	if (resolved === "dark") {
		root.classList.add("dark");
	} else {
		root.classList.remove("dark");
	}
}

export function ThemeProvider({ children }: { children: React.ReactNode }) {
	const [mode, setModeState] = useState<ThemeMode>(getStoredTheme);
	const [systemPref, setSystemPref] = useState<"light" | "dark">(getSystemPreference);

	// Listen for system preference changes
	useEffect(() => {
		const mq = window.matchMedia("(prefers-color-scheme: dark)");
		const handler = (e: MediaQueryListEvent) => {
			setSystemPref(e.matches ? "dark" : "light");
		};
		mq.addEventListener("change", handler);
		return () => mq.removeEventListener("change", handler);
	}, []);

	const resolved = mode === "auto" ? systemPref : mode;

	// Apply dark class to <html>
	useEffect(() => {
		applyTheme(resolved);
	}, [resolved]);

	const setMode = useCallback((newMode: ThemeMode) => {
		setModeState(newMode);
		try {
			localStorage.setItem(THEME_KEY, newMode);
		} catch {
			// Ignore
		}
	}, []);

	const value = useMemo(() => ({ mode, resolved, setMode }), [mode, resolved, setMode]);

	return createElement(ThemeContext.Provider, { value }, children);
}

export function useTheme(): ThemeContextValue {
	const ctx = useContext(ThemeContext);
	if (!ctx) throw new Error("useTheme must be used within ThemeProvider");
	return ctx;
}
