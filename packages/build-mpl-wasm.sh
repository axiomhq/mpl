#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$SCRIPT_DIR/.."
CRATE_DIR="$REPO_ROOT/extra/mpl-language-server-wasm"
DEST_PKG_DIR="$SCRIPT_DIR/mpl-language-server-wasm"

cd "$REPO_ROOT"
# --no-opt: wasm-pack's bundled wasm-opt (v117) crashes on this binary, and even
# system wasm-opt (v126) increases gzipped size despite shrinking raw size, because
# our wasm-release profile (LTO + opt-level=z) already produces compression-friendly output.
wasm-pack build "$CRATE_DIR" --scope axiomhq --target web --profile wasm-release --no-opt
mkdir -p "$DEST_PKG_DIR"
cp -r "$CRATE_DIR/pkg/"* "$DEST_PKG_DIR/"

echo "MPL language-server WASM package built successfully"
