import { describe, it, expect, vi, afterEach } from "vitest";
import * as mpl from "@axiomhq/mpl";
import {
  extractParamAt,
  mergeSystemParamsInto,
  parseParamDeclarations,
  type ParamDecl,
} from "./hover";
import type { MplSystemParam } from "./system-params";

// The `param` declaration grammar is parsed in Rust (wasm
// `param_declarations`, reusing the completion engine's scanner). Parsing
// edge cases — Option<T> unwrapping, whitespace tolerance, missing `;`,
// commented-out lines — are covered by the Rust `declared_params` /
// `extract_declared_params` suites. These tests only exercise the thin TS
// adapter that reshapes the wasm array into the Map the hover source uses,
// with the wasm boundary mocked.
describe("parseParamDeclarations", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("maps wasm declarations into a Map keyed by the $-prefixed name", () => {
    vi.spyOn(mpl, "param_declarations").mockReturnValue([
      { name: "$env", type: "string", optional: false },
      { name: "$container", type: "string", optional: true },
    ]);
    const decls = parseParamDeclarations("param $env: string;\nds:metric");
    expect(decls.size).toBe(2);
    expect(decls.get("$env")).toEqual({ type: "string", optional: false });
    expect(decls.get("$container")).toEqual({ type: "string", optional: true });
  });

  it("returns an empty map when wasm reports no declarations", () => {
    vi.spyOn(mpl, "param_declarations").mockReturnValue([]);
    expect(parseParamDeclarations("ds:metric | where x == 1").size).toBe(0);
  });

  it("returns an empty map when the wasm module is unavailable", () => {
    vi.spyOn(mpl, "param_declarations").mockImplementation(() => {
      throw new Error("wasm not ready");
    });
    expect(parseParamDeclarations("param $env: string;\nds:m").size).toBe(0);
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
    // language server agree on which type wins. The inline decl is built
    // directly (parsing is covered separately) to keep this a focused test
    // of the merge precedence rather than the wasm parser.
    const decls = new Map<string, ParamDecl>([
      ["$__interval", { type: "int", optional: false }],
    ]);
    mergeSystemParamsInto(decls, [{ name: "__interval", type: "Duration" }]);
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
