import { Menu } from "lucide-react";
import { ThemeToggle } from "./theme-toggle";

interface TopBarProps {
	onMenuClick: () => void;
}

export function TopBar({ onMenuClick }: TopBarProps) {
	return (
		<header className="flex items-center justify-between h-14 px-4 bg-surface-1 border-b border-border lg:px-6">
			{/* Mobile hamburger */}
			<button
				type="button"
				onClick={onMenuClick}
				className="p-1.5 rounded-md text-text-secondary hover:bg-surface-3 lg:hidden cursor-pointer"
				aria-label="Open menu"
			>
				<Menu size={20} />
			</button>

			{/* Spacer for desktop (sidebar provides branding) */}
			<div className="hidden lg:block" />

			{/* Right side */}
			<div className="flex items-center gap-3">
				<ThemeToggle />
			</div>
		</header>
	);
}
