import { describe, expect, it } from "vitest";
import { parseParamDeclarations } from "@axiomhq/mpl-codemirror";
import { formatValue, substituteParams } from "./params";

describe("formatValue", () => {
  it("wraps bare values into the literal each type expects", () => {
    expect(formatValue("string", "hello")).toBe('"hello"');
    expect(formatValue("Dataset", "k8s-metrics-dev")).toBe("`k8s-metrics-dev`");
    expect(formatValue("Regex", ".*api.*")).toBe("#/.*api.*/");
    expect(formatValue("Duration", "5m")).toBe("5m");
    expect(formatValue("int", "42")).toBe("42");
    expect(formatValue("bool", "true")).toBe("true");
  });

  it("leaves values that are already literals untouched", () => {
    expect(formatValue("string", '"hi"')).toBe('"hi"');
    expect(formatValue("Dataset", "`ds`")).toBe("`ds`");
    expect(formatValue("Regex", "#/x/")).toBe("#/x/");
  });
});

describe("substituteParams", () => {
  const doc = [
    "param $dataset: Dataset;",
    "param $window: Duration;",
    "$dataset:metric | align to $window using avg",
  ].join("\n");
  const decls = parseParamDeclarations(doc);

  it("replaces references but leaves declaration lines intact", () => {
    const out = substituteParams(doc, decls, { $dataset: "my-ds", $window: "5m" });
    expect(out).toContain("param $dataset: Dataset;");
    expect(out).toContain("`my-ds`:metric | align to 5m using avg");
  });

  it("leaves references with no supplied value alone", () => {
    const out = substituteParams(doc, decls, { $dataset: "my-ds" });
    expect(out).toContain("`my-ds`:metric | align to $window using avg");
  });

  it("skips optional params (handled by ifdef in the interpreter)", () => {
    const optDoc = [
      "param $env: Option<string>;",
      "`ds`:metric | ifdef($env) { where environment == $env }",
    ].join("\n");
    const optDecls = parseParamDeclarations(optDoc);
    const out = substituteParams(optDoc, optDecls, { $env: "prod" });
    expect(out).toContain("ifdef($env) { where environment == $env }");
  });
});
