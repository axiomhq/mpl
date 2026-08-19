import { describe, it, expect } from "vitest";
import { CharCategory, EditorState } from "@codemirror/state";
import { mplWordChars, IFDEF_SNIPPET } from "./completions";

describe("IFDEF_SNIPPET", () => {
  // Importing this constant runs the @codemirror/autocomplete snippet
  // parser at module load. If the template were malformed (unbalanced
  // braces, bad placeholder syntax) the import itself would throw —
  // hence the assertion is also implicitly a parse-time guard.
  it("is exported as a callable suitable for Completion.apply", () => {
    expect(typeof IFDEF_SNIPPET).toBe("function");
    expect(IFDEF_SNIPPET).not.toBe(undefined);
  });
});

describe("mplWordChars", () => {
  it("classifies backtick and $ as word characters", () => {
    const state = EditorState.create({
      doc: "`$a",
      extensions: [mplWordChars],
    });
    const categorize = state.charCategorizer(0);
    expect(categorize("`")).toBe(CharCategory.Word);
    expect(categorize("$")).toBe(CharCategory.Word);
  });

  it("still classifies normal letters and digits as word characters", () => {
    const state = EditorState.create({
      doc: "abc",
      extensions: [mplWordChars],
    });
    const categorize = state.charCategorizer(0);
    expect(categorize("a")).toBe(CharCategory.Word);
    expect(categorize("0")).toBe(CharCategory.Word);
  });

  it("does not classify spaces as word characters", () => {
    const state = EditorState.create({ doc: " ", extensions: [mplWordChars] });
    const categorize = state.charCategorizer(0);
    expect(categorize(" ")).toBe(CharCategory.Space);
  });
});

// `needsEscape`, `escapeIdent` and `applyTextForIdent` delegate to the
// language server; their behaviour is asserted in Rust — see
// `extra/mpl-language-server/src/ident.rs`.
