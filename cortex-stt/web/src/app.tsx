import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter, Route, Routes } from "react-router";
import { ingressBasePath } from "@/lib/ingress";
import { AuthGate } from "./components/auth-gate";
import { AppLayout } from "./components/layout/app-layout";
import { ConfirmProvider } from "./components/ui/confirm";
import { ToastProvider } from "./components/ui/toast";
import { ROUTES } from "./lib/constants";
import { ThemeProvider } from "./lib/theme";
import { DashboardPage } from "./pages/dashboard";
import { EvalPage } from "./pages/eval";
import { HistoryPage } from "./pages/history";
import { KeysPage } from "./pages/keys";
import { ModelsPage } from "./pages/models";
import { SettingsPage } from "./pages/settings";
import { TranscribePage } from "./pages/transcribe";

const queryClient = new QueryClient({
	defaultOptions: {
		queries: {
			staleTime: 5_000,
			retry: 2,
			refetchOnWindowFocus: true,
		},
	},
});

export function App() {
	return (
		<QueryClientProvider client={queryClient}>
			<ThemeProvider>
				<ToastProvider>
					<ConfirmProvider>
						<AuthGate>
							<BrowserRouter basename={ingressBasePath()}>
								<Routes>
									<Route element={<AppLayout />}>
										<Route index element={<DashboardPage />} />
										<Route path={ROUTES.TRANSCRIBE} element={<TranscribePage />} />
										<Route path={ROUTES.MODELS} element={<ModelsPage />} />
										<Route path={ROUTES.HISTORY} element={<HistoryPage />} />
										<Route path={ROUTES.EVAL} element={<EvalPage />} />
										<Route path={ROUTES.KEYS} element={<KeysPage />} />
										<Route path={ROUTES.SETTINGS} element={<SettingsPage />} />
									</Route>
								</Routes>
							</BrowserRouter>
						</AuthGate>
					</ConfirmProvider>
				</ToastProvider>
			</ThemeProvider>
		</QueryClientProvider>
	);
}
