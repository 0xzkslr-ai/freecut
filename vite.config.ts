import { defineConfig } from "vite";

export default defineConfig({
  root: "frontend",
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: { outDir: "../web-dist", emptyOutDir: true, target: "es2022" },
});
