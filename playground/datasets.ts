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

/**
 * The stretch of time a query runs over, in whole seconds: `start` is part of
 * the query, `end` is where it stops. A query service hands the engine one of
 * these per request and exposes it to the query as `$__start` and `$__end`.
 */
export interface QueryWindow {
  start: number;
  end: number;
}

/**
 * The window that covers everything `datasets` carries: the first second any
 * series reports through the second after the last. Data with no samples in it
 * spans nothing.
 */
export function datasetsWindow(datasets: Datasets): QueryWindow {
  let first = Number.POSITIVE_INFINITY;
  let last = Number.NEGATIVE_INFINITY;
  for (const dataset of datasets) {
    for (const metric of dataset.metrics) {
      for (const series of metric.series) {
        for (const timestamp of series.timestamps) {
          if (timestamp < first) first = timestamp;
          if (timestamp > last) last = timestamp;
        }
      }
    }
  }
  if (last < first) return { start: 0, end: 0 };
  return { start: first, end: last + 1 };
}

/** One second as an ISO-8601 UTC instant, to the second. */
function isoSecond(timestamp: number): string {
  return `${new Date(timestamp * 1000).toISOString().slice(0, 19)}Z`;
}

/**
 * The window as a reader sees it: the first instant it covers through the
 * instant it stops at, both in UTC.
 */
export function formatQueryWindow(window: QueryWindow): string {
  return `${isoSecond(window.start)} \u2192 ${isoSecond(window.end)}`;
}

/**
 * The window the playground runs every query in. Pinned to the span of the
 * demo data so each example sees all of its own series.
 */
export const queryWindow: QueryWindow = datasetsWindow(datasets);
