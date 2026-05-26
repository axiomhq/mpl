#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/.."
CRATE_DIR="$REPO_ROOT/extra/mpl-language-server-wasm"
# Published as @axiomhq/mpl on npm. We use --out-name mpl so the file basenames
# (mpl.js, mpl_bg.wasm, …) stay stable for existing consumers, independent of
# the Rust crate name.
DEST_PKG_DIR="$SCRIPT_DIR/mpl"

cd "$REPO_ROOT"
# --no-opt: wasm-pack's bundled wasm-opt (v117) crashes on this binary, and even
# system wasm-opt (v126) increases gzipped size despite shrinking raw size, because
# our wasm-release profile (LTO + opt-level=z) already produces compression-friendly output.
wasm-pack build "$CRATE_DIR" --out-name mpl --scope axiomhq --target web --profile wasm-release --no-opt
mkdir -p "$DEST_PKG_DIR"
cp -r "$CRATE_DIR/pkg/"* "$DEST_PKG_DIR/"

echo "MPL WASM package built successfully"
