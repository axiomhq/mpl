import { defineConfig } from "vitest/config";
import path from "path";

export default defineConfig({
  resolve: {
    alias: {
      // The real package is a WASM build artifact (gitignored). Tests that
      // exercise pure-JS logic don't call any WASM functions, so a stub suffices.
      "@axiomhq/mpl-language-server-wasm": path.resolve(
        __dirname,
        "src/__mpl-language-server-stub__.ts",
      ),
    },
  },
});
