import { describe, it, expect } from "vitest";
import { datasets, datasetsWindow, formatQueryWindow, queryWindow } from "./datasets";

describe("datasetsWindow", () => {
  it("starts at the first sample and stops after the last", () => {
    // The window follows MPL's range semantics — start inclusive, end
    // exclusive — so covering the last sample means ending a second later.
    const data = [
      {
        name: "ds",
        metrics: [
          {
            name: "m",
            series: [
              { name: "a", tags: {}, timestamps: [30, 40], values: [1, 2] },
              { name: "b", tags: {}, timestamps: [10, 20], values: [3, 4] },
            ],
          },
        ],
      },
    ];
    expect(datasetsWindow(data)).toEqual({ start: 10, end: 41 });
  });

  it("spans nothing when no series carries a sample", () => {
    expect(datasetsWindow([])).toEqual({ start: 0, end: 0 });
  });
});

describe("queryWindow", () => {
  it("covers every sample the demo data carries", () => {
    // The playground pins its window to the demo data, so no example loses
    // points to the window it runs in.
    for (const dataset of datasets) {
      for (const metric of dataset.metrics) {
        for (const series of metric.series) {
          for (const timestamp of series.timestamps) {
            expect(timestamp).toBeGreaterThanOrEqual(queryWindow.start);
            expect(timestamp).toBeLessThan(queryWindow.end);
          }
        }
      }
    }
  });
});

describe("formatQueryWindow", () => {
  it("reads as the instants it spans, in UTC", () => {
    // The header shows the window to a reader, so seconds since the epoch
    // become instants — and always UTC, whatever the viewer's timezone.
    expect(formatQueryWindow({ start: 1700000000, end: 1700003600 })).toBe(
      "2023-11-14T22:13:20Z \u2192 2023-11-14T23:13:20Z",
    );
  });
});
