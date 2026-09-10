import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Point the dev proxy at a running server: a local `cargo run` by default,
// or a deployed addon via CORTEX_STT_PROXY=http://<host>:8769.
const apiTarget = process.env.CORTEX_STT_PROXY ?? "http://localhost:10400";

export default defineConfig({
	base: "./",
	plugins: [react()],
	resolve: {
		alias: {
			"@": path.resolve(__dirname, "./src"),
		},
	},
	server: {
		port: 5173,
		proxy: {
			"/api": {
				target: apiTarget,
				changeOrigin: true,
			},
			"/health": {
				target: apiTarget,
				changeOrigin: true,
			},
		},
	},
	build: {
		outDir: "dist",
		sourcemap: false,
		rollupOptions: {
			output: {
				// Function form required since vite 8 moved to rolldown, which
				// does not accept the object form of manualChunks.
				manualChunks(id) {
					if (id.includes("node_modules")) {
						if (id.includes("@tanstack/react-query")) return "query";
						if (id.includes("react-router") || id.includes("react-dom") || id.includes("/react/")) {
							return "vendor";
						}
					}
				},
			},
		},
	},
});
