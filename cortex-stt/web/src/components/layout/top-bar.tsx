import { Menu } from "lucide-react";
import { LiveInput } from "./live-input";
import { ThemeToggle } from "./theme-toggle";

interface TopBarProps {
	onMenuClick: () => void;
}

export function TopBar({ onMenuClick }: TopBarProps) {
	return (
		<header className="flex items-center gap-4 h-[52px] px-4 lg:px-7 bg-surface-1 border-b border-border">
			<button
				type="button"
				onClick={onMenuClick}
				className="p-1.5 rounded-md text-text-secondary hover:bg-surface-3 lg:hidden cursor-pointer"
				aria-label="Open menu"
			>
				<Menu size={20} />
			</button>

			<LiveInput />

			<div className="ml-auto flex items-center gap-3">
				<ThemeToggle />
			</div>
		</header>
	);
}
