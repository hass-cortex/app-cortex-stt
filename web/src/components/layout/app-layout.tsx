import { X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Outlet, useLocation } from "react-router";
import { Sidebar } from "./sidebar";
import { TopBar } from "./top-bar";

export function AppLayout() {
	const [drawerOpen, setDrawerOpen] = useState(false);
	const location = useLocation();

	// Close drawer on route change
	// biome-ignore lint/correctness/useExhaustiveDependencies: intentionally react to pathname changes
	useEffect(() => {
		setDrawerOpen(false);
	}, [location.pathname]);

	const openDrawer = useCallback(() => setDrawerOpen(true), []);
	const closeDrawer = useCallback(() => setDrawerOpen(false), []);

	return (
		<div className="flex h-screen overflow-hidden">
			{/* Desktop sidebar */}
			<div className="hidden lg:flex">
				<Sidebar />
			</div>

			{/* Mobile drawer overlay */}
			{drawerOpen && (
				<div className="fixed inset-0 z-40 lg:hidden">
					{/* Backdrop */}
					<div
						className="fixed inset-0 bg-black/50"
						onClick={closeDrawer}
						onKeyDown={(e) => {
							if (e.key === "Escape") closeDrawer();
						}}
					/>

					{/* Drawer panel */}
					<div className="fixed inset-y-0 left-0 z-50 w-56">
						<div className="relative h-full">
							<Sidebar mobile onNavigate={closeDrawer} />
							<button
								type="button"
								onClick={closeDrawer}
								className="absolute top-3 right-3 p-1 rounded-md text-text-muted hover:text-text-primary hover:bg-surface-3 cursor-pointer"
							>
								<X size={16} />
							</button>
						</div>
					</div>
				</div>
			)}

			{/* Main content area */}
			<div className="flex flex-col flex-1 min-w-0">
				<TopBar onMenuClick={openDrawer} />
				<main className="flex-1 overflow-y-auto p-4 sm:p-6">
					<Outlet />
				</main>
			</div>
		</div>
	);
}
