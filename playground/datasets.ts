// Dataset schema, loading and validation.

import * as v from "valibot";
import rawDatasets from "./datasets.json";

const SeriesSchema = v.object({
  name: v.string(),
  tags: v.record(v.string(), v.string()),
  timestamps: v.array(v.number()),
  values: v.array(v.number()),
});

const MetricSchema = v.object({
  name: v.string(),
  series: v.array(SeriesSchema),
});

const DatasetSchema = v.object({
  name: v.string(),
  metrics: v.array(MetricSchema),
});

const DatasetsSchema = v.array(DatasetSchema);

export type Datasets = v.InferOutput<typeof DatasetsSchema>;

export const datasets: Datasets = v.parse(DatasetsSchema, rawDatasets);
