import { describe, it, expect } from "vitest";
import { parse_steps } from "@axiomhq/mpl-lang";
import { interpret } from "./interpreter";

describe("pipeline end-to-end", () => {
  it("parses and interprets a simple query", () => {
    const { steps } = parse_steps(
      "test:http_requests_total\n| where code == #/[123]../\n| group by method using sum",
    );
    expect(steps.length).toBe(3);
    expect(steps[0].error).toBeUndefined();
    expect(steps[1].error).toBeUndefined();
    expect(steps[2].error).toBeUndefined();

    const result = interpret(steps);
    expect(result.steps.length).toBe(3);
    // Source loaded data
    expect(result.steps[0].length).toBeGreaterThan(0);
  });

  it("throws on syntax errors", () => {
    expect(() => parse_steps("test:http_requests_total\n| blahblah\n| group using sum")).toThrow();
  });

  it("handles compute queries", () => {
    const { steps } = parse_steps(`(
  test:http_requests_total | where code == #/[123]../,
  test:http_requests_total
)
| compute ratio using /`);
    expect(steps.length).toBe(1);
    expect(steps[0].node).toBeDefined();

    const result = interpret(steps);
    expect(result.errors[0]).toContain("Compute");
  });

  it("labels are clean strings", () => {
    const { steps } = parse_steps("// comment\ntest:http_requests_total\n| group using sum");
    expect(steps[0].label).not.toContain("//");
    expect(steps[1].label).toContain("group");
  });

  it("interprets filter + align + group", () => {
    const { steps } = parse_steps(`test:http_requests_total
| where path == #/.*(elastic\\/_bulk|ingest).*/
| where code == #/[123]../
| align to 5m using prom::rate
| group by method, path, code using sum`);
    expect(steps.length).toBe(5);

    const result = interpret(steps);
    expect(result.steps.length).toBe(5);
    // All steps should succeed
    expect(result.errors.every((e) => e === undefined)).toBe(true);
  });

  it("handles unknown function with recovery", () => {
    const { steps } = parse_steps(
      "test:http_requests_total\n| align to 5m using unknown_fn\n| group using sum",
    );
    expect(steps[1].error).toBeDefined();
    // group parsed successfully after recovery
    expect(steps[2].error).toBeUndefined();
  });

  it("handles sample", () => {
    const { steps } = parse_steps("test:http_requests_total\n| sample 0.5\n| group using sum");
    expect(steps.length).toBe(3);
    const result = interpret(steps);
    expect(result.steps.length).toBe(3);
  });
});
