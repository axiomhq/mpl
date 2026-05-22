import { describe, it, expect } from "vitest";
import type { StepOutput, Series } from "@axiomhq/mpl-playground";
import { Interpreter } from "@axiomhq/mpl-playground";
import { datasets } from "./datasets";
import { substituteSystemParams } from "./editor";

const interp = new Interpreter(datasets);

function run(query: string): StepOutput[] {
  return interp.run(query);
}

function ok(r: StepOutput): Series[] {
  if ("Err" in r.result) throw new Error(`Expected Ok, got Err: ${r.result.Err}`);
  return r.result.Ok;
}

function err(r: StepOutput): string {
  if ("Ok" in r.result) throw new Error("Expected Err, got Ok");
  return r.result.Err;
}

describe("parse + interpret end-to-end", () => {
  describe("source loading", () => {
    it("loads existing dataset", () => {
      const result = run("test:http_requests_total");
      expect(result.length).toBe(1);
      expect(ok(result[0]).length).toBeGreaterThan(0);
    });

    it("loads empty for unknown metric", () => {
      const result = run("test:nonexistent_metric");
      expect(ok(result[0]).length).toBe(0);
    });

    it("loads escaped dataset name", () => {
      const result = run("`dev.metrics`:http_requests_total");
      expect(ok(result[0]).length).toBeGreaterThan(0);
    });

    it("errors on parameterized dataset", () => {
      const result = run("param $ds: Dataset;\n$ds:metric");
      expect(err(result[0])).toContain("Parameterized");
    });
  });

  describe("filters", () => {
    it("filters by string equality", () => {
      const result = run('test:http_requests_total\n| where code == "200"');
      const source = ok(result[0]);
      const filtered = ok(result[1]);
      expect(filtered.length).toBeLessThanOrEqual(source.length);
    });

    it("filters by regex", () => {
      const result = run("test:http_requests_total\n| where code == #/[123]../");
      expect(ok(result[1]).length).toBeGreaterThan(0);
    });

    it("filters by numeric comparison", () => {
      const result = run("test:http_requests_total\n| where code >= 500");
      const filtered = ok(result[1]);
      for (const s of filtered) {
        expect(Number(s.tags.get("code"))).toBeGreaterThanOrEqual(500);
      }
    });

    it("errors on unknown tag", () => {
      const result = run('test:http_requests_total\n| where nonexistent == "x"');
      expect(err(result[1])).toContain("Unknown tag");
    });

    it("handles multiple filters", () => {
      const result = run(
        'test:http_requests_total\n| where code == #/[123]../\n| where method == "GET"',
      );
      expect(result.length).toBe(3);
    });
  });

  describe("map operations", () => {
    it("map rate", () => {
      const result = run("test:http_requests_total\n| map rate");
      const series = ok(result[1]);
      expect(series.length).toBeGreaterThan(0);
      // First value is NaN for rate
      expect(series[0].values[0]).toBeNaN();
    });

    it("map multiply", () => {
      const result = run("test:http_requests_total\n| map * 2");
      const source = ok(result[0]);
      const mapped = ok(result[1]);
      expect(mapped[0].values[1]).toBe(source[0].values[1] * 2);
    });

    it("map is::lt", () => {
      const result = run("`com.app.test`:ingest_pressure\n| map is::lt(100)");
      const mapped = ok(result[1]);
      for (const v of mapped[0].values) {
        expect(v === 0 || v === 1).toBe(true);
      }
    });

    it("map abs", () => {
      const result = run("test:http_requests_total\n| map abs");
      const mapped = ok(result[1]);
      for (const v of mapped[0].values) {
        expect(v).toBeGreaterThanOrEqual(0);
      }
    });

    it("map div by zero errors", () => {
      const result = run("test:http_requests_total\n| map / 0");
      expect(err(result[1])).toContain("Division by zero");
    });

    it("map missing arg is a parse error", () => {
      expect(() => run("test:http_requests_total\n| map + ")).toThrow();
    });
  });

  describe("align", () => {
    it("align to 5m using sum", () => {
      const result = run("test:http_requests_total\n| align to 5m using sum");
      const aligned = ok(result[1]);
      expect(aligned.length).toBeGreaterThan(0);
      // Aligned timestamps should be evenly spaced
      const ts = aligned[0].timestamps;
      if (ts.length > 2) {
        const dt = ts[1] - ts[0];
        expect(dt).toBe(300); // 5 minutes = 300 seconds
      }
    });

    it("align using prom::rate", () => {
      const result = run("test:http_requests_total\n| align to 5m using prom::rate");
      expect(ok(result[1]).length).toBeGreaterThan(0);
    });

    it("align using avg", () => {
      const result = run("test:http_requests_total\n| align to 10m using avg");
      expect(ok(result[1]).length).toBeGreaterThan(0);
    });

    it("align using avg for the whole query window", () => {
      const result = run("test:http_requests_total\n| align using avg");
      const aligned = ok(result[1]);
      expect(aligned.length).toBeGreaterThan(0);
      for (const s of aligned) {
        expect(s.timestamps).toHaveLength(1);
        expect(s.values).toHaveLength(1);
      }
    });
  });

  describe("group", () => {
    it("group using sum (all)", () => {
      const result = run("test:http_requests_total\n| group using sum");
      const grouped = ok(result[1]);
      expect(grouped.length).toBe(1);
    });

    it("group by tag using sum", () => {
      const result = run("test:http_requests_total\n| group by method using sum");
      const grouped = ok(result[1]);
      expect(grouped.length).toBeGreaterThan(0);
      for (const s of grouped) {
        expect(s.tags.has("method")).toBe(true);
      }
    });

    it("group by multiple tags", () => {
      const result = run("test:http_requests_total\n| group by method, code using sum");
      const grouped = ok(result[1]);
      for (const s of grouped) {
        expect(s.tags.has("method")).toBe(true);
        expect(s.tags.has("code")).toBe(true);
      }
    });

    it("group empty series errors", () => {
      const result = run("test:nonexistent\n| group using sum");
      expect(err(result[1])).toContain("Cannot group empty");
    });
  });

  describe("bucket", () => {
    it("bucket with histogram percentile", () => {
      const result = run(
        "test:http_request_duration_seconds_bucket\n| bucket by method, path to 5m using interpolate_delta_histogram(0.90, max, 0.99)",
      );
      expect(result.length).toBe(2);
    });

    it("bucket without tags", () => {
      const result = run(
        "test:http_server_request_duration_seconds_bucket\n| bucket to 1m using interpolate_delta_histogram(0.99)",
      );
      expect(result.length).toBe(2);
    });

    it("bucket over the whole query window", () => {
      const result = run(
        "test:http_server_request_duration_seconds_bucket\n| bucket using interpolate_delta_histogram(0.99)",
      );
      const bucketed = ok(result[1]);
      expect(bucketed.length).toBeGreaterThan(0);
      for (const s of bucketed) {
        expect(s.timestamps).toHaveLength(1);
        expect(s.values).toHaveLength(1);
      }
    });
  });

  describe("as", () => {
    it("renames series", () => {
      const result = run("test:http_requests_total\n| as renamed");
      const series = ok(result[1]);
      for (const s of series) {
        expect(s.name).toBe("renamed");
      }
    });
  });

  describe("sample", () => {
    it("reduces series count", () => {
      const result = run("test:http_requests_total\n| sample 0.01");
      const source = ok(result[0]);
      const sampled = ok(result[1]);
      expect(sampled.length).toBeLessThanOrEqual(source.length);
    });
  });

  describe("compute", () => {
    it("returns not supported error", () => {
      const result = run(`(
  test:http_requests_total | where code == #/[123]../,
  test:http_requests_total
)
| compute ratio using /`);
      expect(err(result[0])).toContain("Compute");
    });
  });

  describe("error recovery", () => {
    // The playground rejects time units the in-memory interpreter cannot
    // honour (Month, Year). The query parses cleanly via `compile()` but
    // bails at interpret time, exercising the "carry forward on step error"
    // property through the align path rather than the map path.
    it("unsupported time unit step passes through", () => {
      const result = run(
        "test:http_requests_total\n| align to 1M using avg\n| group using sum",
      );
      expect("Err" in result[1].result).toBe(true);
      // Group still runs on carried-forward series
      expect(result.length).toBe(3);
      expect("Ok" in result[2].result).toBe(true);
    });

    it("interpret error does not stop later steps", () => {
      const result = run("test:http_requests_total\n| map / 0\n| map * 2");
      expect("Err" in result[1].result).toBe(true);
      // map * 2 runs on carried-forward series
      expect("Ok" in result[2].result).toBe(true);
    });
  });

  describe("full pipelines from examples", () => {
    it("align-rate.mpl", () => {
      const result = run(
        "`dev.metrics`:http_requests_total\n| filter path == #/.*(elastic\\/_bulk|ingest|(?:v1\\/(traces|logs|metrics))).*/\n| filter code == #/[123]../\n| align to 5m using prom::rate\n| group by method, path, code using sum",
      );
      expect(result.length).toBe(5);
    });

    it("slo.mpl", () => {
      const result = run(
        '`com.app.test`:ingest_pressure\n| where time_window == "1m"\n| where service == #/axiomdb-[a-f]/\n| group using max\n| map is::lt(100)\n| align to 10m using avg',
      );
      expect(result.length).toBe(6);
    });

    it("max-threshold.mpl", () => {
      const result = run(
        "test:http_request_duration_seconds_count\n| group by path using min\n| map filter::gt(150)",
      );
      expect(result.length).toBe(3);
    });

    it("parser-error.mpl (map * 5)", () => {
      const result = run(
        "`dev.metrics`:alertmanager_alerts\n| map * 5\n| align to 5m using max\n| group using max",
      );
      expect(result.length).toBe(4);
      expect(result.every((r) => "Ok" in r.result)).toBe(true);
    });

    it("set.mpl (directives ignored)", () => {
      const result = run("set strict;\nset x = 42;\nsample_dataset:metric");
      expect(result.length).toBe(1);
    });
  });

  // The playground does not provide runtime values for optional params, so an
  // `ifdef` step should always pass the upstream series through unchanged
  // (rather than evaluating its inner filter, which would fail when it tries
  // to read the unbound param).
  describe("ifdef step", () => {
    it("skips the gated filter and propagates the source series", () => {
      const result = run(
        'param $container: Option<string>;\n\ntest:http_requests_total\n| ifdef($container) { where method == $container }',
      );
      expect(result.length).toBe(2);
      const source = ok(result[0]);
      const afterIfdef = ok(result[1]);
      expect(afterIfdef.length).toBe(source.length);
    });

    // The playground binds no optional params, so an `ifdef ... else` block
    // must run the else branch. Without this, a query that supplies an else
    // filter would silently no-op in the playground while firing in prod —
    // exactly the kind of drift the playground integration tests guard against.
    it("runs the else branch when the gating param is unbound", () => {
      const result = run(
        'param $container: Option<string>;\n\ntest:http_requests_total\n| ifdef($container) { where container == $container } else { where container == "web-1" }',
      );
      expect(result.length).toBe(2);
      const source = ok(result[0]);
      const afterIfdef = ok(result[1]);
      // Else branch filters down to a strict subset of the source series
      // (web-1 only), so we must drop *some* series but not all.
      expect(afterIfdef.length).toBeGreaterThan(0);
      expect(afterIfdef.length).toBeLessThan(source.length);
      // `tags` round-trips through serde_wasm_bindgen as a JS Map, not a
      // plain object, so reach in with `get` rather than property access.
      expect(
        afterIfdef.every(
          (s) =>
            (s.tags as unknown as Map<string, string>).get("container") === "web-1",
        ),
      ).toBe(true);
    });
  });

  describe("system params", () => {
    // The whole point of the playground's `substituteSystemParams` helper
    // is that the interpreter — which has no binding step — receives a
    // query with `$__interval` already resolved. These tests exercise the
    // same pipeline `main.ts` runs on every editor change, so a regression
    // that breaks the live preview shows up here first.
    it("resolves $__interval before the interpreter sees it", () => {
      const doc = "test:http_requests_total | align to $__interval using avg";
      const resolved = substituteSystemParams(doc);
      const result = run(resolved);
      expect(result.length).toBe(2);
      const aligned = ok(result[1]);
      expect(aligned.length).toBeGreaterThan(0);
    });

    it("runs without errors through a bucket step using $__interval", () => {
      const doc =
        "test:http_requests_total | bucket to $__interval using histogram(0.5, 0.95)";
      const result = run(substituteSystemParams(doc));
      // Source + bucket = 2 steps; both must produce series, neither errors.
      expect(result.length).toBe(2);
      ok(result[0]);
      ok(result[1]);
    });

    // Pins the user-reported failure: prom::rate at 1m on data sampled at
    // 1m cadence, then grouped, used to bail with "No valid points at
    // column N for Sum". The fix has two parts that this test exercises
    // together — trailing-window rate + group tolerance for all-NaN
    // columns — so a regression in either would fail this case.
    it("runs prom::rate + group by end-to-end at $__interval cadence", () => {
      const doc = `\`dev.metrics\`:http_requests_total
| filter path == #/.*(elastic\\/_bulk|ingest|(?:v1\\/(traces|logs|metrics))).*/
| filter code == #/[123]../
| align to $__interval using prom::rate
| group by method, path, code using sum`;
      const result = run(substituteSystemParams(doc));
      for (const step of result) {
        if ("Err" in step.result) {
          throw new Error(`step "${step.text}" failed: ${step.result.Err}`);
        }
      }
      // The final grouped series must carry at least one finite value;
      // an all-NaN result would mean the chart panel renders blank.
      const last = result[result.length - 1];
      const series = ok(last);
      expect(series.length).toBeGreaterThan(0);
      const finite = series[0].values.filter(v => Number.isFinite(v)).length;
      expect(finite).toBeGreaterThan(0);
    });
  });
});
