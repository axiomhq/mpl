#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MPL_LIB_DIR="$SCRIPT_DIR/.."
DEST_PKG_DIR="$SCRIPT_DIR/mpl"

cd "$MPL_LIB_DIR"
# --no-opt: wasm-pack's bundled wasm-opt (v117) crashes on this binary, and even
# system wasm-opt (v126) increases gzipped size despite shrinking raw size, because
# our wasm-release profile (LTO + opt-level=z) already produces compression-friendly output.
wasm-pack build --scope axiomhq --target web --profile wasm-release --no-opt -- --no-default-features --features playground
mkdir -p "$DEST_PKG_DIR"
cp -r pkg/* "$DEST_PKG_DIR/"

echo "MPL WASM package built successfully"
