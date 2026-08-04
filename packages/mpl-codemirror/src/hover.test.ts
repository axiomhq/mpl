import { describe, it, expect } from "vitest";
import { mergeSystemParamsInto, type ParamDecl } from "./hover";
import type { MplSystemParam } from "./system-params";

// `parseParamDeclarations` and the token lookup behind hover delegate to the
// language server; their behaviour is asserted in Rust — see
// `extra/mpl-language-server/src/tokenize/tests.rs` (`token_at`) and
// `completions/tests.rs` (param declarations). Against the wasm stub they
// return nothing, so what is covered here is the pure-TS merge logic.

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
    const decls = new Map<string, ParamDecl>([
      ["$__interval", { type: "int", optional: false }],
    ]);
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
