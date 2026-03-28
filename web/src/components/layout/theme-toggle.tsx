import { type ThemeMode, useTheme } from "@/lib/theme";
import { Monitor, Moon, Sun } from "lucide-react";

const modes: { mode: ThemeMode; icon: typeof Sun; label: string }[] = [
	{ mode: "light", icon: Sun, label: "Light" },
	{ mode: "dark", icon: Moon, label: "Dark" },
	{ mode: "auto", icon: Monitor, label: "Auto" },
];

export function ThemeToggle() {
	const { mode, setMode } = useTheme();

	return (
		<div className="flex items-center bg-surface-3 rounded-lg p-0.5">
			{modes.map(({ mode: m, icon: Icon, label }) => (
				<button
					key={m}
					type="button"
					onClick={() => setMode(m)}
					title={label}
					className={`p-1.5 rounded-md transition-colors cursor-pointer ${
						mode === m
							? "bg-surface-2 text-accent shadow-sm"
							: "text-text-muted hover:text-text-secondary"
					}`}
				>
					<Icon size={14} />
				</button>
			))}
		</div>
	);
}
