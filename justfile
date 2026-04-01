set export

# shows help
_default:
    @echo "Run Metrics stack locally"
    @echo
    @just --list --unsorted
    @echo
    @echo "Execute recipes with just <recipe>"


wasm-test:
    # --no-opt: wasm-pack's bundled wasm-opt (v117) crashes on this binary, and even
    # system wasm-opt (v126) increases gzipped size despite shrinking raw size, because
    # our wasm-release profile (LTO + opt-level=z) already produces compression-friendly output.
    wasm-pack build --target web --profile wasm-release --no-opt --no-default-features --features wasm
    npx tsc --noEmit -t esnext mpl.d.ts

# launch the MPL Playground in a dev server, requires wasm-pack and node/npm installed
playground:
  npm install
  npm run build:mpl
  npm run dev

# remove all generated files (Rust target, wasm-pack output, npm installs, and built packages)
clean:
  cargo clean
  npm run clean
