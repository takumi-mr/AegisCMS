import { resolve } from "node:path";
import vue from "@vitejs/plugin-vue";
import { defineConfig } from "vite";

// Build a single pre-compiled runtime-only IIFE bundle + CSS file into ../assets/
// so AegisCMS serves them at /plugins/<plugin-name>/assets/island.{js,css}
// without requiring 'unsafe-eval' under Strict CSP.
export default defineConfig({
  plugins: [vue()],
  define: {
    "process.env.NODE_ENV": JSON.stringify("production"),
  },
  build: {
    outDir: resolve(__dirname, "../assets"),
    emptyOutDir: true,
    cssCodeSplit: false,
    lib: {
      entry: resolve(__dirname, "src/main.ts"),
      name: "AegisVueIsland",
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
