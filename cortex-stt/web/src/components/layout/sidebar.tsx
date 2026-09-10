import {
	ChevronLeft,
	ChevronRight,
	FlaskConical,
	History,
	Key,
	LayoutDashboard,
	LogOut,
	Mic,
	Package,
	Settings,
} from "lucide-react";
import { useCallback, useState } from "react";
import { NavLink } from "react-router";
import { setApiKey } from "@/api/client";
import { CortexLogo } from "@/components/ui/cortex-logo";
import { useEvalOverview } from "@/hooks/use-eval";
import { useModels } from "@/hooks/use-models";
import { useHealth } from "@/hooks/use-system";
import { ROUTES, SIDEBAR_COLLAPSED_KEY } from "@/lib/constants";
import { isIngress } from "@/lib/ingress";

interface NavItem {
	path: string;
	label: string;
	icon: typeof LayoutDashboard;
	/** Count shown at the end of the row — a standing fact about the
	 *  section, not a notification. */
	badge?: number;
	badgeTone?: "muted" | "accent";
}

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
	const { data: health } = useHealth();
	const { data: models } = useModels();
	const { data: evalOverview } = useEvalOverview();

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

	const installed = (models ?? []).filter(
		(m) => m.status === "downloaded" || m.status === "custom",
	).length;
	const pending = evalOverview?.pending_count ?? 0;

	const navItems: NavItem[] = [
		{ path: ROUTES.DASHBOARD, label: "Dashboard", icon: LayoutDashboard },
		{ path: ROUTES.TRANSCRIBE, label: "Transcribe", icon: Mic },
		{
			path: ROUTES.MODELS,
			label: "Models",
			icon: Package,
			badge: installed || undefined,
			badgeTone: "muted",
		},
		{ path: ROUTES.HISTORY, label: "History", icon: History },
		{
			path: ROUTES.EVAL,
			label: "Evaluation",
			icon: FlaskConical,
			badge: pending || undefined,
			badgeTone: "accent",
		},
		{ path: ROUTES.KEYS, label: "API Keys", icon: Key },
		{ path: ROUTES.SETTINGS, label: "Settings", icon: Settings },
	];

	const isCollapsed = mobile ? false : collapsed;

	return (
		<aside
			className={`flex flex-col bg-surface-1 border-r border-border h-full transition-all duration-200 ${
				isCollapsed ? "w-16" : "w-[216px]"
			} ${mobile ? "w-[216px]" : ""}`}
		>
			<div className="flex items-center justify-between h-[52px] px-4 border-b border-border">
				{!isCollapsed && (
					<div className="flex items-center gap-2.5 overflow-hidden">
						<CortexLogo size={22} className="shrink-0" />
						<span className="text-[13px] font-semibold tracking-[0.02em] text-text-primary truncate">
							Cortex STT
						</span>
					</div>
				)}
				{isCollapsed && <CortexLogo size={22} className="mx-auto" />}
				{!mobile && (
					<button
						type="button"
						onClick={toggleCollapsed}
						className="p-1 rounded-md text-text-muted hover:text-text-primary hover:bg-surface-3 transition-colors cursor-pointer"
						aria-label={isCollapsed ? "Expand sidebar" : "Collapse sidebar"}
					>
						{isCollapsed ? <ChevronRight size={16} /> : <ChevronLeft size={16} />}
					</button>
				)}
			</div>

			<nav className="flex-1 py-3 px-2.5 space-y-0.5 overflow-y-auto">
				{navItems.map((item) => (
					<NavLink
						key={item.path}
						to={item.path}
						end={item.path === "/"}
						onClick={onNavigate}
						className={({ isActive }) =>
							`flex items-center gap-2.5 px-2.5 py-2 rounded-md text-[13px] transition-colors ${
								isActive
									? "bg-accent-wash text-text-primary font-medium shadow-[inset_2px_0_0_var(--accent)]"
									: "text-text-secondary hover:bg-surface-3 hover:text-text-primary"
							} ${isCollapsed ? "justify-center" : ""}`
						}
						aria-label={isCollapsed ? item.label : undefined}
					>
						{({ isActive }) => (
							<>
								<item.icon
									size={17}
									strokeWidth={1.6}
									className={`shrink-0 ${isActive ? "text-accent-ink" : "text-text-muted"}`}
								/>
								{!isCollapsed && (
									<>
										<span className="truncate">{item.label}</span>
										{item.badge !== undefined && (
											<span
												className={`num ml-auto text-[10.5px] px-[5px] rounded-[3px] ${
													item.badgeTone === "accent"
														? "bg-accent-wash text-accent-ink"
														: "text-text-faint"
												}`}
											>
												{item.badge}
											</span>
										)}
									</>
								)}
							</>
						)}
					</NavLink>
				))}
			</nav>

			<div className="px-4 py-3.5 border-t border-border space-y-1.5">
				<div className="flex items-center gap-[7px]">
					<span
						className={`w-1.5 h-1.5 rounded-full ${
							health?.status === "ok" ? "bg-success" : "bg-warning"
						}`}
					/>
					<span className="num text-[11px] text-text-secondary">
						{health?.status === "ok" ? "engine ready" : (health?.status ?? "connecting")}
					</span>
				</div>
				{!isCollapsed && (
					<div className="flex items-center justify-between">
						<span className="num text-[11px] text-text-faint">v{health?.version ?? "—"}</span>
						{!isIngress() && (
							<button
								type="button"
								onClick={() => {
									setApiKey(null);
									window.location.reload();
								}}
								className="flex items-center gap-1.5 text-[11px] text-text-faint hover:text-text-primary transition-colors cursor-pointer"
							>
								<LogOut size={13} strokeWidth={1.6} />
								Sign out
							</button>
						)}
					</div>
				)}
			</div>
		</aside>
	);
}
