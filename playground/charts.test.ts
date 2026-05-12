import { describe, it, expect } from "vitest";
import { formatScalar, isScalarEntry, type ChartEntry } from "./charts";
import type { Series } from "@axiomhq/mpl-playground";

function series(name: string, points: Array<[number, number]>): Series {
  // Cast through unknown — `Series` from the playground crate carries tag
  // metadata we don't need for these pure-logic tests.
  return {
    name,
    timestamps: points.map(([t]) => t),
    values: points.map(([, v]) => v),
    tags: {},
  } as unknown as Series;
}

describe("isScalarEntry", () => {
  // Pins the trigger condition for tile rendering. Drift here would either
  // produce wasted single-dot charts (false negative) or hide multi-point
  // series behind a single tile (false positive).
  it("is true when every series has exactly one timestamp", () => {
    const entry: ChartEntry = {
      label: "step",
      series: [series("a", [[1, 10]]), series("b", [[1, 20]])],
    };
    expect(isScalarEntry(entry)).toBe(true);
  });

  it("is false when any series has more than one timestamp", () => {
    const entry: ChartEntry = {
      label: "step",
      series: [
        series("a", [[1, 10]]),
        series("b", [
          [1, 20],
          [2, 30],
        ]),
      ],
    };
    expect(isScalarEntry(entry)).toBe(false);
  });

  it("is false for an empty series list — nothing to render as a tile", () => {
    const entry: ChartEntry = { label: "step", series: [] };
    expect(isScalarEntry(entry)).toBe(false);
  });

  it("is false when a series has zero timestamps (no data)", () => {
    // Distinct from the scalar case: a series with no points should keep
    // hitting the existing 'No data points' empty path, not a blank tile.
    const entry: ChartEntry = {
      label: "step",
      series: [series("a", [])],
    };
    expect(isScalarEntry(entry)).toBe(false);
  });
});

describe("formatScalar", () => {
  it("renders integers without a decimal point", () => {
    expect(formatScalar(42)).toBe("42");
    expect(formatScalar(0)).toBe("0");
    expect(formatScalar(-7)).toBe("-7");
  });

  it("renders fractional numbers with two decimals", () => {
    expect(formatScalar(1.2345)).toBe("1.23");
    expect(formatScalar(-0.5)).toBe("-0.50");
  });

  it("uses scientific notation for very large magnitudes", () => {
    // Bound the tile width — a 12-digit integer would blow the layout.
    expect(formatScalar(1.5e12)).toBe("1.50e+12");
  });

  it("uses scientific notation for very small non-zero magnitudes", () => {
    expect(formatScalar(1.5e-6)).toBe("1.50e-6");
  });

  it("passes NaN and Infinity through verbatim so they remain legible", () => {
    expect(formatScalar(NaN)).toBe("NaN");
    expect(formatScalar(Infinity)).toBe("Infinity");
    expect(formatScalar(-Infinity)).toBe("-Infinity");
  });
});
