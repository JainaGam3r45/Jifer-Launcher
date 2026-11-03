import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Tauri serves the build from disk and proxies the dev server; a fixed port keeps `devUrl` stable.
export default defineConfig({
  root: "src",
  publicDir: false,
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    fs: { allow: [".."] }
  },
  envPrefix: ["VITE_"],
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false
  }
});
