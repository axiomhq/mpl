/**
 * Wasm integration test for --features playground build.
 *
 * Uses parse_steps() — only exported under --features playground — to verify:
 *   - tests/examples/*.mpl  → parse without error (or produce only
 *                             "not_supported" / "not_implemented" step errors,
 *                             mirroring the tolerance in tests/parse.rs)
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

if (typeof mpl.parse_steps !== "function") {
  console.error(
    "ERROR: parse_steps() not exported — was the build run with --features playground?"
  );
  process.exit(1);
}

// ---------------------------------------------------------------------------

// Errors that mean "parsed OK but the backend doesn't support it yet".
// These are acceptable in tests/examples/ — same tolerance as tests/parse.rs.
function isAcceptableError(msg) {
  return (
    msg.includes("not_supported") ||
    msg.includes("not supported") ||
    msg.includes("not_implemented") ||
    msg.includes("not implemented") ||
    msg.includes("Not implemented")
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

// --- examples: must parse (step errors only for not_supported/not_implemented) -

const examplesDir = join(repoRoot, "tests/examples");
console.log(`\nExamples (must parse) — ${examplesDir}`);

for (const { name, path } of mplFiles(examplesDir)) {
  const content = readFileSync(path, "utf8");
  try {
    const result = mpl.parse_steps(content);
    const errors = result.steps
      .filter((s) => s.error)
      .map((s) => s.error);

    if (errors.length === 0) {
      console.log(`  PASS  ${name}`);
      passed++;
    } else if (errors.every(isAcceptableError)) {
      console.log(`  PASS  ${name}  (parsed; feature not yet supported)`);
      passed++;
    } else {
      console.error(`  FAIL  ${name}: ${errors[0]}`);
      failed++;
    }
  } catch (err) {
    console.error(`  FAIL  ${name}: ${String(err).split("\n")[0]}`);
    failed++;
  }
}

// --- errors: must throw a hard parse error ----------------------------------

const errorsDir = join(repoRoot, "tests/errors");
console.log(`\nErrors (must throw) — ${errorsDir}`);

for (const { name, path } of mplFiles(errorsDir)) {
  const content = readFileSync(path, "utf8");
  try {
    const result = mpl.parse_steps(content);
    const hasStepError = result.steps.some((s) => s.error);
    if (hasStepError) {
      console.log(`  PASS  ${name}  (step error)`);
      passed++;
    } else {
      console.error(
        `  FAIL  ${name}: expected a parse error but parsed successfully`
      );
      failed++;
    }
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
