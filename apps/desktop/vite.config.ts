import { defineConfig } from "vite";

export default defineConfig({
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { target: "esnext", rollupOptions: { input: { index: "index.html", settings: "settings.html" } } },
});
