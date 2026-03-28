import { ROUTES, SIDEBAR_COLLAPSED_KEY } from "@/lib/constants";
import {
	Activity,
	ChevronLeft,
	ChevronRight,
	Cpu,
	History,
	Key,
	LayoutDashboard,
	Package,
	Settings,
} from "lucide-react";
import { useCallback, useState } from "react";
import { NavLink } from "react-router";

interface NavItem {
	path: string;
	label: string;
	icon: typeof LayoutDashboard;
}

const navItems: NavItem[] = [
	{ path: ROUTES.DASHBOARD, label: "Dashboard", icon: LayoutDashboard },
	{ path: ROUTES.MODELS, label: "Models", icon: Package },
	{ path: ROUTES.ENGINE, label: "Engine", icon: Cpu },
	{ path: ROUTES.HISTORY, label: "History", icon: History },
	{ path: ROUTES.KEYS, label: "API Keys", icon: Key },
	{ path: ROUTES.SETTINGS, label: "Settings", icon: Settings },
];

function getInitialCollapsed(): boolean {
	try {
		return localStorage.getItem(SIDEBAR_COLLAPSED_KEY) === "true";
	} catch {
		return false;
	}
}

interface SidebarProps {
	mobile?: boolean;
	onNavigate?: () => void;
}

export function Sidebar({ mobile = false, onNavigate }: SidebarProps) {
	const [collapsed, setCollapsed] = useState(getInitialCollapsed);

	const toggleCollapsed = useCallback(() => {
		setCollapsed((prev) => {
			const next = !prev;
			try {
				localStorage.setItem(SIDEBAR_COLLAPSED_KEY, String(next));
			} catch {
				// Ignore
			}
			return next;
		});
	}, []);

	// Mobile sidebar is always expanded
	const isCollapsed = mobile ? false : collapsed;

	return (
		<aside
			className={`flex flex-col bg-surface-1 border-r border-border h-full transition-all duration-200 ${
				isCollapsed ? "w-16" : "w-56"
			} ${mobile ? "w-56" : ""}`}
		>
			{/* Header */}
			<div className="flex items-center justify-between h-14 px-3 border-b border-border">
				{!isCollapsed && (
					<div className="flex items-center gap-2 overflow-hidden">
						<Activity size={20} className="text-accent shrink-0" />
						<span className="text-sm font-semibold text-text-primary truncate">Wyoming ASR</span>
					</div>
				)}
				{isCollapsed && <Activity size={20} className="text-accent mx-auto" />}
				{!mobile && (
					<button
						type="button"
						onClick={toggleCollapsed}
						className="p-1 rounded-md text-text-muted hover:text-text-primary hover:bg-surface-3 transition-colors cursor-pointer"
						title={isCollapsed ? "Expand sidebar" : "Collapse sidebar"}
					>
						{isCollapsed ? <ChevronRight size={16} /> : <ChevronLeft size={16} />}
					</button>
				)}
			</div>

			{/* Navigation */}
			<nav className="flex-1 py-2 px-2 space-y-0.5 overflow-y-auto">
				{navItems.map((item) => (
					<NavLink
						key={item.path}
						to={item.path}
						end={item.path === "/"}
						onClick={onNavigate}
						className={({ isActive }) =>
							`flex items-center gap-2.5 px-2.5 py-2 rounded-lg text-sm font-medium transition-colors ${
								isActive
									? "bg-accent/10 text-accent"
									: "text-text-secondary hover:bg-surface-3 hover:text-text-primary"
							} ${isCollapsed ? "justify-center" : ""}`
						}
						title={isCollapsed ? item.label : undefined}
					>
						<item.icon size={18} className="shrink-0" />
						{!isCollapsed && <span className="truncate">{item.label}</span>}
					</NavLink>
				))}
			</nav>

			{/* Footer */}
			{!isCollapsed && (
				<div className="px-3 py-3 border-t border-border">
					<p className="text-[10px] text-text-muted">Wyoming ASR v0.1.0</p>
				</div>
			)}
		</aside>
	);
}
