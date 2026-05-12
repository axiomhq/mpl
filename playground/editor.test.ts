import { describe, it, expect } from "vitest";
import { substituteSystemParams, SYSTEM_PARAM_FACET } from "./editor";

describe("substituteSystemParams", () => {
  it("replaces a registered $__interval reference with its concrete value", () => {
    const input = "test:m | align to $__interval using avg";
    expect(substituteSystemParams(input)).toBe(
      "test:m | align to 1m using avg",
    );
  });

  it("leaves unrelated text untouched", () => {
    const input = "test:m | where tag == \"prod\"";
    expect(substituteSystemParams(input)).toBe(input);
  });

  it("substitutes every occurrence in the document", () => {
    // Multiple uses in one query must all get rewritten so the interpreter
    // sees no remaining `$__interval` references.
    const input =
      "test:m | align to $__interval using avg | bucket to $__interval using histogram(0.99)";
    expect(substituteSystemParams(input)).toBe(
      "test:m | align to 1m using avg | bucket to 1m using histogram(0.99)",
    );
  });

  it("respects a right word boundary so similarly-named identifiers are not corrupted", () => {
    // `$__interval_extended` must not be rewritten to `1m_extended` — the
    // boundary anchor on the right prevents the substring match.
    const input = "test:m | where tag == $__interval_extended";
    expect(substituteSystemParams(input)).toBe(input);
  });

  it("returns the input unchanged when no system params are referenced", () => {
    const input = "test:m";
    expect(substituteSystemParams(input)).toBe(input);
  });
});

describe("SYSTEM_PARAM_FACET", () => {
  it("strips the value field so it matches the MplSystemParam shape", () => {
    // The facet payload must carry only `name`, `type`, and optionally
    // `optional` — leaking `value` would clutter the language-server API
    // surface with playground-specific data.
    for (const entry of SYSTEM_PARAM_FACET) {
      expect(entry).not.toHaveProperty("value");
      expect(entry).toHaveProperty("name");
      expect(entry).toHaveProperty("type");
    }
  });

  it("includes the __interval registration the playground exercises", () => {
    const names = SYSTEM_PARAM_FACET.map(p => p.name);
    expect(names).toContain("__interval");
  });
});
