import { ViewPlugin, ViewUpdate, Decoration, DecorationSet, EditorView } from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";
import * as mpl from "@axiomhq/mpl";

// NOTE: All MPL grammar knowledge now lives in Rust (`mpl_lang::wparser`), which
// is compiled to WASM and exposed as `mpl.tokenize`. The lexer is *total* — it
// returns sorted, non-overlapping, gap-free tokens (including `comment`) even
// for incomplete / mid-edit input — so this file no longer duplicates the
// grammar as regexes. There is no regex fallback to keep in sync anymore.

type TokenType =
  | "variable"
  | "string"
  | "number"
  | "bool"
  | "regexp"
  | "operator"
  | "punctuation"
  | "keyword"
  | "type"
  | "comment";

interface Token {
  from: number;
  to: number;
  type: TokenType;
}

const decos: Record<TokenType, Decoration> = {
  keyword: Decoration.mark({ class: "mpl-keyword" }),
  variable: Decoration.mark({ class: "mpl-variable" }),
  string: Decoration.mark({ class: "mpl-string" }),
  number: Decoration.mark({ class: "mpl-number" }),
  bool: Decoration.mark({ class: "mpl-bool" }),
  regexp: Decoration.mark({ class: "mpl-regexp" }),
  operator: Decoration.mark({ class: "mpl-operator" }),
  punctuation: Decoration.mark({ class: "mpl-punctuation" }),
  type: Decoration.mark({ class: "mpl-type" }),
  comment: Decoration.mark({ class: "mpl-comment" }),
};

function buildDecorations(view: EditorView): DecorationSet {
  const doc = view.state.doc.toString();

  let tokens: Token[] = [];
  try {
    tokens = (mpl.tokenize(doc) as Token[] | null | undefined) ?? [];
  } catch {
    // WASM not loaded yet — leave the document undecorated; the next view
    // update will retry once `mpl` is ready.
    tokens = [];
  }

  // The Rust lexer already emits ascending, non-overlapping spans, so we can
  // feed the builder directly. The guard only defends against an unexpected
  // shape (e.g. a future API change) corrupting the RangeSet.
  const builder = new RangeSetBuilder<Decoration>();
  let last = 0;
  for (const t of tokens) {
    const deco = decos[t.type];
    if (!deco || t.from < last || t.to <= t.from) continue;
    builder.add(t.from, t.to, deco);
    last = t.to;
  }
  return builder.finish();
}

export const mplHighlighter = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = buildDecorations(view);
    }

    update(update: ViewUpdate) {
      if (update.docChanged || update.viewportChanged) {
        this.decorations = buildDecorations(update.view);
      }
    }
  },
  { decorations: (v) => v.decorations },
);
