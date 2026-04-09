/**
 * Wasm integration test for --features playground build.
 *
 * Uses Interpreter.run() to verify:
 *   - tests/examples/*.mpl  → compile without error, except known
 *                             unsupported/not implemented syntax
 *                             mirrored from tests/parse.rs.
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
  : join(repoRoot, "extra/mpl-playground/pkg");

const mpl = await import(join(pkgDir, "mpl_playground.js"));
const wasmBytes = readFileSync(join(pkgDir, "mpl_playground_bg.wasm"));
mpl.initSync({ module: wasmBytes });

if (typeof mpl.Interpreter !== "function") {
  console.error(
    "ERROR: Interpreter not exported — was mpl-playground built?"
  );
  process.exit(1);
}

const interpreter = new mpl.Interpreter([]);

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
    interpreter.run(content);
    console.log(`  PASS  ${name}`);
    passed++;
  } catch (err) {
    const msg = String(err).split("\n")[0];
    if (isAcceptableError(msg)) {
      console.log(`  PASS  ${name}  (feature not yet supported)`);
      passed++;
    } else {
      console.error(`  FAIL  ${name}: ${msg}`);
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
    interpreter.run(content);
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
