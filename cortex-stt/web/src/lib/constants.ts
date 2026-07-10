/** Route path constants */
export const ROUTES = {
	DASHBOARD: "/",
	MODELS: "/models",
	HISTORY: "/history",
	KEYS: "/keys",
	SETTINGS: "/settings",
} as const;

/** TanStack Query key factories */
export const queryKeys = {
	health: ["health"] as const,
	system: {
		all: ["system"] as const,
		hardware: () => [...queryKeys.system.all, "hardware"] as const,
		metrics: () => [...queryKeys.system.all, "metrics"] as const,
		storage: () => [...queryKeys.system.all, "storage"] as const,
	},
	models: {
		all: ["models"] as const,
		list: () => [...queryKeys.models.all, "list"] as const,
		detail: (id: string) => [...queryKeys.models.all, "detail", id] as const,
	},
	engine: {
		all: ["engine"] as const,
		status: () => [...queryKeys.engine.all, "status"] as const,
	},
	history: {
		all: ["history"] as const,
		list: (filters?: Record<string, string>) =>
			[...queryKeys.history.all, "list", filters ?? {}] as const,
		detail: (id: string) => [...queryKeys.history.all, "detail", id] as const,
		facets: () => [...queryKeys.history.all, "facets"] as const,
	},
	keys: {
		all: ["keys"] as const,
		list: () => [...queryKeys.keys.all, "list"] as const,
	},
	settings: {
		all: ["settings"] as const,
	},
} as const;

/** Sidebar collapse state localStorage key */
export const SIDEBAR_COLLAPSED_KEY = "cortex-stt-sidebar-collapsed";

/** Theme preference localStorage key */
export const THEME_KEY = "cortex-stt-theme";

/** Default polling intervals (ms) */
export const POLL_INTERVALS = {
	HEALTH: 30_000,
	METRICS: 10_000,
	ENGINE_STATUS: 5_000,
	DOWNLOAD_PROGRESS: 1_000,
} as const;
