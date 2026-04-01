import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';
import path from 'path';

export default defineConfig({
  plugins: [
    wasm(),
    topLevelAwait()
  ],
  resolve: {
    alias: {
      '@axiomhq/mpl-codemirror/styles/tokens.css': path.resolve(__dirname, '../packages/mpl-codemirror/styles/tokens.css'),
      '@axiomhq/mpl-codemirror': path.resolve(__dirname, '../packages/mpl-codemirror/src/index.ts'),
      '@axiomhq/mpl-lang': path.resolve(__dirname, '../packages/mpl/mpl_lang.js'),
    },
    dedupe: [
      '@codemirror/state',
      '@codemirror/view',
      '@codemirror/language',
      '@codemirror/autocomplete',
      '@codemirror/lint',
    ]
  },
  server: {
    open: true,
    fs: {
      allow: ['.', '../packages']
    }
  }
});
