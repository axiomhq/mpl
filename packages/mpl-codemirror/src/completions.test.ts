import { describe, it, expect } from "vitest";
import { CharCategory, EditorState } from "@codemirror/state";
import { needsEscape, escapeIdent, applyTextForIdent, mplWordChars } from "./completions";

describe("mplWordChars", () => {
  it("classifies backtick and $ as word characters", () => {
    const state = EditorState.create({ doc: "`$a", extensions: [mplWordChars] });
    const categorize = state.charCategorizer(0);
    expect(categorize("`")).toBe(CharCategory.Word);
    expect(categorize("$")).toBe(CharCategory.Word);
  });

  it("still classifies normal letters and digits as word characters", () => {
    const state = EditorState.create({ doc: "abc", extensions: [mplWordChars] });
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
    expect(needsEscape("axiom-dev")).toBe(true);
  });

  it("returns true for names with dots", () => {
    expect(needsEscape("axiom-dev.metrics")).toBe(true);
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
    expect(escapeIdent("axiom-dev.metrics")).toBe("`axiom-dev.metrics`");
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
      expect(applyTextForIdent("axiom-dev.metrics", false)).toBe("`axiom-dev.metrics`");
    });
  });

  describe("when in backtick context (user already typed opening backtick)", () => {
    it("appends closing backtick for names needing escaping", () => {
      // User typed: `axi  (span.from is after the backtick)
      // CodeMirror replaces from span.from with apply text
      // Result: `axiom-dev.metrics`  (original backtick + apply text)
      expect(applyTextForIdent("axiom-dev.metrics", true)).toBe("axiom-dev.metrics`");
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
  //   applyTextForIdent("axiom-dev.metrics", true) => "axiom-dev.metrics`"
  //   CodeMirror replaces doc[1..4] with "axiom-dev.metrics`"
  //   Result: "`axiom-dev.metrics`"  (correct, single backtick pair)
  it("avoids double backtick in the exact bug scenario", () => {
    const doc = "`axi";
    const from = 1; // Rust span.from skips the backtick
    const inBacktick = from > 0 && doc.charAt(from - 1) === "`";
    expect(inBacktick).toBe(true);

    const apply = applyTextForIdent("axiom-dev.metrics", inBacktick);
    expect(apply).toBe("axiom-dev.metrics`");

    // Simulate CodeMirror replacement: doc[0..from] + apply
    const result = doc.slice(0, from) + apply;
    expect(result).toBe("`axiom-dev.metrics`");
  });

  // Simulates bare text (no backtick typed yet)
  it("adds full backtick wrapping when no backtick in document", () => {
    const doc = "axi";
    const from = 0;
    const inBacktick = from > 0 && doc.charAt(from - 1) === "`";
    expect(inBacktick).toBe(false);

    const apply = applyTextForIdent("axiom-dev.metrics", inBacktick);
    expect(apply).toBe("`axiom-dev.metrics`");

    const result = doc.slice(0, from) + apply;
    expect(result).toBe("`axiom-dev.metrics`");
  });
});
