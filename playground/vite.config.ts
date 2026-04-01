import fs from "node:fs";
import path from "node:path";
import { defineConfig } from "vite";
import wasm from "vite-plugin-wasm";

function generateExamplesIndex() {
  const dir = path.resolve(import.meta.dirname, "public/examples");
  const files = fs
    .readdirSync(dir)
    .filter((f) => f.endsWith(".mpl"))
    .sort();
  fs.writeFileSync(path.join(dir, "index.json"), JSON.stringify(files, null, 2));
}

function examplesIndexPlugin() {
  return {
    name: "examples-index",
    buildStart() {
      generateExamplesIndex();
    },
    configureServer(server: {
      watcher: {
        add(dir: string): void;
        on(event: string, cb: (type: string, path: string) => void): void;
      };
    }) {
      const dir = path.resolve(import.meta.dirname, "public/examples");
      server.watcher.add(dir);
      server.watcher.on("all", (_: string, filePath: string) => {
        if (filePath.startsWith(dir) && filePath.endsWith(".mpl")) {
          generateExamplesIndex();
        }
      });
    },
  };
}

export default defineConfig({
  plugins: [wasm(), examplesIndexPlugin()],
  test: {
    setupFiles: ["./vitest.setup.ts"],
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
      "@axiomhq/mpl-lang": path.resolve(import.meta.dirname, "../packages/mpl/mpl_lang.js"),
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
