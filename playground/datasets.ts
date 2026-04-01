// Dataset schema, loading and validation.

import * as v from "valibot";

export const SeriesSchema = v.object({
  name: v.string(),
  tags: v.record(v.string(), v.string()),
  timestamps: v.array(v.number()),
  values: v.array(v.number()),
});

export type Series = v.InferOutput<typeof SeriesSchema>;

const DatasetsSchema = v.record(v.string(), v.record(v.string(), v.array(SeriesSchema)));

type Datasets = v.InferOutput<typeof DatasetsSchema>;
type DatasetIndex = Record<string, Record<string, string[]>>;

let db: Datasets = {};

export async function initDatasets(): Promise<void> {
  const resp = await fetch("/datasets.json");
  if (!resp.ok) return;
  db = v.parse(DatasetsSchema, await resp.json());
}

export function initDatasetsSync(data: unknown): void {
  db = v.parse(DatasetsSchema, data);
}

export function loadDatasetIndex(): DatasetIndex {
  const index: DatasetIndex = {};
  for (const [dataset, metrics] of Object.entries(db)) {
    index[dataset] = {};
    for (const [metric, series] of Object.entries(metrics)) {
      const tags = new Set<string>();
      for (const s of series) {
        for (const k of Object.keys(s.tags)) tags.add(k);
      }
      index[dataset][metric] = [...tags].sort();
    }
  }
  return index;
}

export function loadSeries(dataset: string, metric: string): Series[] {
  return db[dataset]?.[metric] ?? [];
}
