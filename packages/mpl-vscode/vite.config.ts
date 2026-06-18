import { defineConfig } from "vite";

export default defineConfig({
  build: {
    emptyOutDir: true,
    outDir: "dist",
    rollupOptions: {
      external: [/^node:/, "vscode"],
      output: {
        entryFileNames: "extension.js",
        format: "cjs",
      },
    },
    ssr: "src/extension.ts",
    target: "node20",
  },
  ssr: {
    noExternal: true,
  },
});
