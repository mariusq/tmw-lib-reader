import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/target/**"] },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: { target: "es2021" },
  test: { environment: "jsdom", globals: true, setupFiles: ["./src/test/setup.ts"] },
});
