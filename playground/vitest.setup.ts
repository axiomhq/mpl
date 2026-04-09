import fs from "node:fs";
import path from "node:path";
import { initSync } from "@axiomhq/mpl-lang";

const wasmPath = path.resolve(import.meta.dirname, "../packages/mpl/mpl_lang_bg.wasm");
const wasmBytes = fs.readFileSync(wasmPath);
initSync({ module: wasmBytes });
