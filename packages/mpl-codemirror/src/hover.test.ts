import { describe, it, expect } from "vitest";
import {
  extractParamAt,
  mergeSystemParamsInto,
  parseParamDeclarations,
  type ParamDecl,
} from "./hover";
import type { MplSystemParam } from "./system-params";

describe("parseParamDeclarations", () => {
  it("returns an empty map for a doc with no params", () => {
    const decls = parseParamDeclarations("ds:metric | where x == 1");
    expect(decls.size).toBe(0);
  });

  it("parses a simple non-optional declaration", () => {
    const decls = parseParamDeclarations("param $env: string;\nds:metric");
    expect(decls.get("$env")).toEqual({ type: "string", optional: false });
  });

  it("parses an Option<T> declaration and unwraps the inner type", () => {
    const decls = parseParamDeclarations(
      "param $container: Option<string>;\nds:metric",
    );
    expect(decls.get("$container")).toEqual({ type: "string", optional: true });
  });

  it("collects multiple declarations into a single map", () => {
    const decls = parseParamDeclarations(
      "param $ds: Dataset;\nparam $w: Duration;\nparam $f: Option<int>;\nds:m",
    );
    expect(decls.size).toBe(3);
    expect(decls.get("$ds")?.optional).toBe(false);
    expect(decls.get("$w")?.type).toBe("Duration");
    expect(decls.get("$f")).toEqual({ type: "int", optional: true });
  });

  it("tolerates extra whitespace around colon and Option brackets", () => {
    const decls = parseParamDeclarations(
      "param $a :  Option<  Regex  >  ;\nds:m",
    );
    expect(decls.get("$a")).toEqual({ type: "Regex", optional: true });
  });

  it("ignores incomplete declarations missing a semicolon", () => {
    // Mid-typing: no `;` yet — parser must not match a partial line
    const decls = parseParamDeclarations("param $env: string\nds:metric");
    expect(decls.size).toBe(0);
  });

  it("ignores commented-out declarations", () => {
    const decls = parseParamDeclarations(
      "// param $shadowed: string;\nparam $real: int;\nds:m",
    );
    expect(decls.has("$shadowed")).toBe(false);
    expect(decls.get("$real")?.type).toBe("int");
  });
});

describe("extractParamAt", () => {
  // Document layout (offsets):
  //   "where tag == $container and"
  //    0    5  9  12 13         24
  // `$` at 13, `container` at 14..23.
  const doc = "where tag == $container and";
  const dollar = doc.indexOf("$");
  const lastNameChar = dollar + "$container".length - 1;

  it("matches when the cursor is on the `$` itself", () => {
    const r = extractParamAt(doc, dollar);
    expect(r).toEqual({ name: "$container", from: dollar, to: dollar + 10 });
  });

  it("matches when the cursor is on a letter mid-name", () => {
    const r = extractParamAt(doc, dollar + 3); // on `o` of $container
    expect(r?.name).toBe("$container");
  });

  it("matches when the cursor is on the last name char", () => {
    const r = extractParamAt(doc, lastNameChar);
    expect(r?.name).toBe("$container");
  });

  it("returns null when the cursor is on whitespace", () => {
    const r = extractParamAt(doc, dollar - 1); // space before `$`
    expect(r).toBeNull();
  });

  it("returns null when the cursor is on a non-param identifier", () => {
    const r = extractParamAt(doc, doc.indexOf("tag")); // `tag` is not a param
    expect(r).toBeNull();
  });

  it("returns null for a bare `$` with no identifier following", () => {
    const r = extractParamAt("where tag == $", "where tag == ".length);
    expect(r).toBeNull();
  });

  it("handles a param at the very start of the document", () => {
    const r = extractParamAt("$ds:metric", 0);
    expect(r).toEqual({ name: "$ds", from: 0, to: 3 });
  });

  it("returns null for an out-of-range position", () => {
    expect(extractParamAt("$x", 99)).toBeNull();
    expect(extractParamAt("", 0)).toBeNull();
  });
});

describe("mergeSystemParamsInto", () => {
  it("adds system params to an empty declaration map", () => {
    const decls = new Map<string, ParamDecl>();
    const sys: MplSystemParam[] = [{ name: "__interval", type: "Duration" }];
    mergeSystemParamsInto(decls, sys);
    expect(decls.get("$__interval")).toEqual({
      type: "Duration",
      optional: false,
    });
  });

  it("normalises names supplied without a leading $", () => {
    // Hosts that store names internally without `$` shouldn't have to add
    // it; the merge layer matches whatever `extractParamAt` produces, which
    // always carries the prefix.
    const decls = new Map<string, ParamDecl>();
    mergeSystemParamsInto(decls, [{ name: "__interval", type: "Duration" }]);
    expect(decls.has("$__interval")).toBe(true);
    expect(decls.has("__interval")).toBe(false);
  });

  it("accepts names that already carry the $ prefix", () => {
    const decls = new Map<string, ParamDecl>();
    mergeSystemParamsInto(decls, [{ name: "$__interval", type: "Duration" }]);
    expect(decls.has("$__interval")).toBe(true);
  });

  it("does not overwrite an inline declaration of the same name", () => {
    // Inline `param` declarations win on name collision — same precedence
    // the completion source enforces, so hover, completion, and the
    // language server agree on which type wins.
    const decls = parseParamDeclarations("param $__interval: int;\nds:m");
    mergeSystemParamsInto(decls, [
      { name: "__interval", type: "Duration" },
    ]);
    expect(decls.get("$__interval")?.type).toBe("int");
  });

  it("carries the optional flag onto the resulting ParamDecl", () => {
    const decls = new Map<string, ParamDecl>();
    mergeSystemParamsInto(decls, [
      { name: "__env", type: "string", optional: true },
    ]);
    expect(decls.get("$__env")).toEqual({ type: "string", optional: true });
  });
});
