import { resolve } from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Build a single self-contained IIFE bundle + CSS file into ../assets/
// so AegisCMS serves them at /plugins/<plugin-name>/assets/island.{js,css}
// under Strict CSP (script-src 'self'; style-src 'self').
export default defineConfig({
  plugins: [react()],
  define: {
    "process.env.NODE_ENV": JSON.stringify("production"),
  },
  build: {
    outDir: resolve(__dirname, "../assets"),
    emptyOutDir: true,
    cssCodeSplit: false,
    lib: {
      entry: resolve(__dirname, "src/main.tsx"),
      name: "AegisReactIsland",
      formats: ["iife"],
      fileName: () => "island.js",
    },
    rollupOptions: {
      output: {
        assetFileNames: (info) =>
          info.names?.some((n) => n.endsWith(".css")) ? "island.css" : "[name][extname]",
      },
    },
  },
});
