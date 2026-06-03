// End-to-end check of the REAL completion source against the REAL wasm and the
// playground datasets — the path the playground actually runs, which the
// codemirror unit tests stub out. Reproduces the two reported autocomplete bugs.
import fs from "node:fs";
import path from "node:path";
import { beforeAll, describe, expect, it } from "vitest";
import { initSync } from "@axiomhq/mpl";
import { EditorState } from "@codemirror/state";
import { CompletionContext } from "@codemirror/autocomplete";
import { createMplCompletionSource } from "../packages/mpl-codemirror/src/completions";
import { mplSystemParams } from "../packages/mpl-codemirror/src/system-params";
import { datasets } from "./datasets";

beforeAll(() => {
  const wasmPath = path.resolve(import.meta.dirname, "../packages/mpl/mpl_bg.wasm");
  initSync({ module: fs.readFileSync(wasmPath) });
});

const config = {
  datasets: async () => datasets.map((ds) => ds.name),
  metrics: async (dataset: string) =>
    datasets.find((ds) => ds.name === dataset)?.metrics.map((m) => m.name) ?? [],
  tags: async (dataset: string, metric: string) => {
    const series =
      datasets.find((ds) => ds.name === dataset)?.metrics.find((m) => m.name === metric)?.series ??
      [];
    const tags = new Set<string>();
    for (const s of series) for (const k of Object.keys(s.tags)) tags.add(k);
    return [...tags].sort();
  },
};

async function complete(doc: string, sys: { name: string; type: string }[] = []) {
  const source = createMplCompletionSource(config);
  const state = EditorState.create({
    doc,
    extensions: [mplSystemParams.of(sys as never)],
  });
  const ctx = new CompletionContext(state, doc.length, false);
  return await source(ctx);
}

// Every example query opens with a `//` comment header; the source extractor
// must see through it, or tag completion silently returns nothing.
const HEADER =
  "// Tag-vs-tag comparison: a tag can be referenced as a value, so the RHS of a\n" +
  "// filter can be another tag instead of a constant.\n";

describe("playground completion e2e (real wasm + datasets)", () => {
  it("bug1: tag-vs-tag RHS suggests other tags (with comment header)", async () => {
    const r = await complete(HEADER + "service_mesh:mesh_request_count\n| where src_region != d");
    const labels = r?.options.map((o) => o.label) ?? [];
    expect(labels).toContain("dst_region");
  });

  it("bug2: interpolation suggests both params and tags (with comment header)", async () => {
    const r = await complete(
      HEADER + 'service_mesh:mesh_request_count\n| where src_region != "${',
      [{ name: "__interval", type: "Duration" }],
    );
    const labels = r?.options.map((o) => o.label) ?? [];
    expect(labels).toContain("$__interval");
    expect(labels).toContain("dst_region");
  });
});
