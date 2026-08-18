// Release-time helper for VSIX packaging.
//
// Copies a prebuilt `mpl-lsp` binary into this extension's `server/` directory
// so `vsce package` includes it. By default, it stages Cargo's local release
// binary from `target/release`; cross-target builds can pass an explicit binary
// path as the first CLI argument. This script is not run when a packaged
// extension is installed or activated.

import { copyFileSync, chmodSync, existsSync, mkdirSync, rmSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageRoot = dirname(fileURLToPath(new URL("../package.json", import.meta.url)));
const repoRoot = resolve(packageRoot, "../..");
const defaultExecutableName = process.platform === "win32" ? "mpl-lsp.exe" : "mpl-lsp";
const defaultSource = join(repoRoot, "target", "release", defaultExecutableName);
const source = process.argv[2] ?? defaultSource;

const sourcePath = resolve(source);

if (!existsSync(sourcePath)) {
  console.error(`MPL LSP binary does not exist: ${sourcePath}`);
  process.exit(1);
}

const serverDir = join(packageRoot, "server");
const executableName = basename(sourcePath).endsWith(".exe")
  ? "mpl-lsp.exe"
  : "mpl-lsp";
const destination = join(serverDir, executableName);

rmSync(serverDir, { recursive: true, force: true });
mkdirSync(serverDir, { recursive: true });
copyFileSync(sourcePath, destination);

if (!destination.endsWith(".exe")) {
  chmodSync(destination, 0o755);
}

console.log(`Staged ${destination}`);
