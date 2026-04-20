import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

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
				target: "http://localhost:10400",
				changeOrigin: true,
			},
			"/health": {
				target: "http://localhost:10400",
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
