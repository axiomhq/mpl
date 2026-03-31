/**
 * Wasm integration test for --features playground build.
 *
 * Uses parse_json() — only exported under --features playground — to verify:
 *   - tests/examples/*.mpl  → parse without error (or throw only with
 *                             "not_supported" / "not_implemented", mirroring
 *                             the tolerance in tests/parse.rs)
 *   - tests/errors/*.mpl    → throw a hard parse error
 *
 * Usage: node tests/wasm/test-playground.mjs [pkg-dir]
 *   pkg-dir defaults to "pkg" (relative to repo root)
 */

import { readFileSync, readdirSync } from "fs";
import { resolve, dirname, join } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../..");
const pkgDir = process.argv[2]
  ? resolve(process.argv[2])
  : join(repoRoot, "pkg");

const mpl = await import(join(pkgDir, "mpl_lang.js"));
const wasmBytes = readFileSync(join(pkgDir, "mpl_lang_bg.wasm"));
mpl.initSync({ module: wasmBytes });

if (typeof mpl.parse_json !== "function") {
  console.error(
    "ERROR: parse_json() not exported — was the build run with --features playground?"
  );
  process.exit(1);
}

// ---------------------------------------------------------------------------

// Strip ANSI escape codes so we can match on the plain error identifier.
const ANSI_RE = /\x1B\[[0-9;]*m/g;
function stripAnsi(s) {
  return String(s).replace(ANSI_RE, "");
}

// Errors that mean "parsed OK but the backend doesn't support it yet".
// These are acceptable in tests/examples/ — same tolerance as tests/parse.rs.
function isAcceptableExampleError(err) {
  const msg = stripAnsi(err);
  return (
    msg.startsWith("mpl_lang::not_supported") ||
    msg.startsWith("mpl_lang::not_implemented")
  );
}

let passed = 0;
let failed = 0;

function mplFiles(dir) {
  return readdirSync(dir)
    .filter((f) => f.endsWith(".mpl"))
    .sort()
    .map((f) => ({ name: f, path: join(dir, f) }));
}

// --- examples: must parse (or fail only with not_supported/not_implemented) -

const examplesDir = join(repoRoot, "tests/examples");
console.log(`\nExamples (must parse) — ${examplesDir}`);

for (const { name, path } of mplFiles(examplesDir)) {
  const content = readFileSync(path, "utf8");
  try {
    mpl.parse_json(content);
    console.log(`  PASS  ${name}`);
    passed++;
  } catch (err) {
    if (isAcceptableExampleError(err)) {
      console.log(`  PASS  ${name}  (parsed; feature not yet supported)`);
      passed++;
    } else {
      console.error(`  FAIL  ${name}: ${stripAnsi(err).split("\n")[0]}`);
      failed++;
    }
  }
}

// --- errors: must throw a hard parse error ----------------------------------

const errorsDir = join(repoRoot, "tests/errors");
console.log(`\nErrors (must throw) — ${errorsDir}`);

for (const { name, path } of mplFiles(errorsDir)) {
  const content = readFileSync(path, "utf8");
  try {
    mpl.parse_json(content);
    console.error(
      `  FAIL  ${name}: expected a parse error but parsed successfully`
    );
    failed++;
  } catch (_err) {
    console.log(`  PASS  ${name}`);
    passed++;
  }
}

// --- summary ----------------------------------------------------------------

console.log(`\n${passed + failed} tests: ${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
