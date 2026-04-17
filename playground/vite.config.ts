import path from "node:path";
import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";

export default defineConfig({
  plugins: [wasm()],
  test: {
    setupFiles: ["./vitest.setup.ts"],
    exclude: ["e2e/**"],
  },
  resolve: {
    alias: {
      "@axiomhq/mpl-codemirror/styles/tokens.css": path.resolve(
        import.meta.dirname,
        "../packages/mpl-codemirror/styles/tokens.css",
      ),
      "@axiomhq/mpl-codemirror": path.resolve(
        import.meta.dirname,
        "../packages/mpl-codemirror/src/index.ts",
      ),
      "@axiomhq/mpl": path.resolve(import.meta.dirname, "../packages/mpl/mpl_lang.js"),
      "@axiomhq/mpl-playground": path.resolve(import.meta.dirname, "wasm/mpl_playground.js"),
    },
    dedupe: [
      "@codemirror/state",
      "@codemirror/view",
      "@codemirror/language",
      "@codemirror/autocomplete",
      "@codemirror/lint",
    ],
  },
  server: {
    open: true,
    fs: {
      allow: [".", "../packages"],
    },
  },
});
