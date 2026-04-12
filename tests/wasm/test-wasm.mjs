/**
 * Wasm integration test for --features wasm (non-playground build).
 *
 * Uses diagnostics() — always exported under --features wasm — to verify:
 *   - tests/examples/*.mpl  → no hard parse errors (errors with "not supported" /
 *                             "not implemented" messages are acceptable, mirroring
 *                             the tolerance in tests/parse.rs)
 *   - tests/errors/*.mpl    → at least one error diagnostic
 *
 * Usage: node tests/wasm/test-wasm.mjs [pkg-dir]
 *   pkg-dir defaults to "pkg" (relative to repo root)
 */

import { readFileSync, readdirSync } from "fs";
import { resolve, dirname, join } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../..");
const pkgDir = process.argv[2]
  ? resolve(process.argv[2])
  : join(repoRoot, "crates/mpl-lang/pkg");

const mpl = await import(join(pkgDir, "mpl_lang.js"));
const wasmBytes = readFileSync(join(pkgDir, "mpl_lang_bg.wasm"));
mpl.initSync({ module: wasmBytes });

if (typeof mpl.diagnostics !== "function") {
  console.error("ERROR: diagnostics() not exported — wrong build artifact?");
  process.exit(1);
}

// ---------------------------------------------------------------------------

// Errors whose message starts with one of these prefixes mean "parsed OK but
// the backend feature is not available yet" — same tolerance as tests/parse.rs.
const ACCEPTABLE_PREFIXES = [
  "This feature is not supported at the moment",
  "Not implemented:",
];

function isAcceptable(diag) {
  return ACCEPTABLE_PREFIXES.some((p) => diag.message.startsWith(p));
}

let passed = 0;
let failed = 0;

function mplFiles(dir) {
  return readdirSync(dir)
    .filter((f) => f.endsWith(".mpl"))
    .sort()
    .map((f) => ({ name: f, path: join(dir, f) }));
}

// --- examples: must have no hard errors ------------------------------------

const examplesDir = join(repoRoot, "crates/mpl-lang/tests/examples");
console.log(`\nExamples (must parse) — ${examplesDir}`);

for (const { name, path } of mplFiles(examplesDir)) {
  const content = readFileSync(path, "utf8");
  const diags = mpl.diagnostics(content);
  const hardErrors = diags.filter(
    (d) => d.severity === "error" && !isAcceptable(d)
  );

  if (hardErrors.length === 0) {
    const note = diags.some((d) => d.severity === "error")
      ? "  (parsed; feature not yet supported)"
      : "";
    console.log(`  PASS  ${name}${note}`);
    passed++;
  } else {
    console.error(`  FAIL  ${name}: ${hardErrors[0].message}`);
    failed++;
  }
}

// --- errors: must have at least one error diagnostic -----------------------

const errorsDir = join(repoRoot, "crates/mpl-lang/tests/errors");
console.log(`\nErrors (must have errors) — ${errorsDir}`);

for (const { name, path } of mplFiles(errorsDir)) {
  const content = readFileSync(path, "utf8");
  const diags = mpl.diagnostics(content);
  const errors = diags.filter((d) => d.severity === "error");

  if (errors.length > 0) {
    console.log(`  PASS  ${name}`);
    passed++;
  } else {
    console.error(`  FAIL  ${name}: expected errors but got none`);
    failed++;
  }
}

// --- summary ----------------------------------------------------------------

console.log(`\n${passed + failed} tests: ${passed} passed, ${failed} failed`);
if (failed > 0) {
  process.exit(1);
}
