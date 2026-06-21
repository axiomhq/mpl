import { ViewPlugin, ViewUpdate, Decoration, DecorationSet, EditorView } from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";
import * as mpl from "@axiomhq/mpl";

// Token types produced by the Rust tokenizer (mpl-language-server `tokenize`,
// which walks the rowan CST). This list is the *whole* highlighter contract:
// the MPL grammar lives in Rust, not here.
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

// Highlighting is driven entirely by the Rust CST tokenizer. There is no JS
// grammar fallback any more: the rowan parser recovers from incomplete /
// invalid input, so `tokenize` returns the tokens it could recognise even
// mid-edit (comments and trivia included). The only failure mode is the WASM
// module not being initialised yet, in which case we render no decorations
// until the next update re-runs after init.
function buildDecorations(view: EditorView): DecorationSet {
  const doc = view.state.doc.toString();

  let tokens: Token[] | null = null;
  try {
    tokens = mpl.tokenize(doc) as Token[] | null;
  } catch {
    tokens = null;
  }

  const builder = new RangeSetBuilder<Decoration>();
  if (!tokens) return builder.finish();

  // The Rust tokenizer guarantees ascending, non-overlapping spans, which is
  // exactly what RangeSetBuilder requires.
  for (const t of tokens) {
    const deco = decos[t.type];
    if (deco && t.from < t.to) {
      builder.add(t.from, t.to, deco);
    }
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
