import fs from "node:fs";
import path from "node:path";
import { initSync } from "@axiomhq/mpl-playground";

const wasmPath = path.resolve(import.meta.dirname, "wasm/mpl_playground_bg.wasm");
const wasmBytes = fs.readFileSync(wasmPath);
initSync({ module: wasmBytes });
