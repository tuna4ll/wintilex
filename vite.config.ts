import { defineConfig } from "vite";

// Tauri serves the frontend from a fixed port during development and reads the
// built assets from dist/ in release.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 5183,
    strictPort: true,
  },
  build: {
    target: "esnext",
    outDir: "dist",
    emptyOutDir: true,
  },
});
