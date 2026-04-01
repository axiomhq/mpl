# MPL playground

Try out MPL in the browser

Make sure your system has the following:

- rust toolchain: https://rustup.rs/
- wasm-pack: https://github.com/drager/wasm-pack
- node and npm: https://nodejs.org/en/download
- (optional e2e tests) playwright and chromium: https://playwright.dev/docs/browsers

From the repo root, running `npm run dev` will compile the Rust `lib/mpl` crate into wasm, create a wrapper NPM package with the JavaScript glue and TypeScript definitions and copy it to `packages/mpl`.
