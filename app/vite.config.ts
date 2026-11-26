/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed dev port and serves the build from ../dist.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "chrome120", outDir: "dist", emptyOutDir: true },
  test: { environment: "node", include: ["src/**/*.test.ts"], isolate: false },
});
