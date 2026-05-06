import { describe, it, expect } from "vitest";
import { CharCategory, EditorState } from "@codemirror/state";
import {
  needsEscape,
  escapeIdent,
  applyTextForIdent,
  mplWordChars,
  IFDEF_SNIPPET,
} from "./completions";

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

describe("needsEscape", () => {
  it("returns false for plain identifiers", () => {
    expect(needsEscape("cpu")).toBe(false);
    expect(needsEscape("myMetric")).toBe(false);
    expect(needsEscape("a1")).toBe(false);
    expect(needsEscape("foo_bar")).toBe(false);
  });

  it("returns true for names with hyphens", () => {
    expect(needsEscape("metrixs-dev")).toBe(true);
  });

  it("returns true for names with dots", () => {
    expect(needsEscape("dev.metrics")).toBe(true);
  });

  it("returns true for names with spaces", () => {
    expect(needsEscape("my app")).toBe(true);
  });

  it("returns true for names starting with a digit", () => {
    expect(needsEscape("1foo")).toBe(true);
  });

  it("returns true for empty string", () => {
    expect(needsEscape("")).toBe(true);
  });
});

describe("escapeIdent", () => {
  it("returns plain names unchanged", () => {
    expect(escapeIdent("cpu")).toBe("cpu");
  });

  it("wraps names needing escaping in backticks", () => {
    expect(escapeIdent("dev.metrics")).toBe("`dev.metrics`");
  });

  it("escapes backticks inside the name", () => {
    expect(escapeIdent("has`tick")).toBe("`has\\`tick`");
  });

  it("escapes backslashes inside the name", () => {
    expect(escapeIdent("has\\slash")).toBe("`has\\\\slash`");
  });
});

describe("applyTextForIdent", () => {
  describe("when NOT in backtick context (user typed bare text)", () => {
    it("returns plain name unchanged", () => {
      expect(applyTextForIdent("cpu", false)).toBe("cpu");
    });

    it("wraps names needing escaping in backticks", () => {
      expect(applyTextForIdent("dev.metrics", false)).toBe("`dev.metrics`");
    });
  });

  describe("when in backtick context (user already typed opening backtick)", () => {
    it("appends closing backtick for names needing escaping", () => {
      // User typed: `axi  (span.from is after the backtick)
      // CodeMirror replaces from span.from with apply text
      // Result: `dev.metrics`  (original backtick + apply text)
      expect(applyTextForIdent("dev.metrics", true)).toBe("dev.metrics`");
    });

    it("appends closing backtick even for plain names", () => {
      // User typed: `cp  — the backtick is already in the document,
      // so we must close it regardless of whether the name needs escaping
      expect(applyTextForIdent("cpu", true)).toBe("cpu`");
    });

    it("escapes backticks in the name and appends closing backtick", () => {
      expect(applyTextForIdent("has`tick", true)).toBe("has\\`tick`");
    });
  });

  // Simulates the exact double-backtick bug scenario:
  // User types: `axi
  //   Rust span: { from: 1, to: 4 }  (skips opening backtick)
  //   doc.charAt(0) === '`'  =>  inBacktick = true
  //   applyTextForIdent("dev.metrics", true) => "dev.metrics`"
  //   CodeMirror replaces doc[1..4] with "dev.metrics`"
  //   Result: "`dev.metrics`"  (correct, single backtick pair)
  it("avoids double backtick in the exact bug scenario", () => {
    const doc = "`axi";
    const from = 1; // Rust span.from skips the backtick
    const inBacktick = from > 0 && doc.charAt(from - 1) === "`";
    expect(inBacktick).toBe(true);

    const apply = applyTextForIdent("dev.metrics", inBacktick);
    expect(apply).toBe("dev.metrics`");

    // Simulate CodeMirror replacement: doc[0..from] + apply
    const result = doc.slice(0, from) + apply;
    expect(result).toBe("`dev.metrics`");
  });

  // Simulates bare text (no backtick typed yet)
  it("adds full backtick wrapping when no backtick in document", () => {
    const doc = "axi";
    const from = 0;
    const inBacktick = from > 0 && doc.charAt(from - 1) === "`";
    expect(inBacktick).toBe(false);

    const apply = applyTextForIdent("dev.metrics", inBacktick);
    expect(apply).toBe("`dev.metrics`");

    const result = doc.slice(0, from) + apply;
    expect(result).toBe("`dev.metrics`");
  });
});
