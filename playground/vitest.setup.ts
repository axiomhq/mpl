import fs from "node:fs";
import path from "node:path";
import { initSync } from "@axiomhq/mpl-lang";
import { initDatasetsSync } from "./datasets";

const wasmPath = path.resolve(import.meta.dirname, "../packages/mpl/mpl_lang_bg.wasm");
const wasmBytes = fs.readFileSync(wasmPath);
initSync({ module: wasmBytes });

const datasetsPath = path.resolve(import.meta.dirname, "public/datasets.json");
const datasetsJson = JSON.parse(fs.readFileSync(datasetsPath, "utf-8"));
initDatasetsSync(datasetsJson);
