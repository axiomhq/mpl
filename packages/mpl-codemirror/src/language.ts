import { ViewPlugin, ViewUpdate, Decoration, DecorationSet, EditorView } from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";
import * as mpl from "@axiomhq/mpl";

type TokenType =
  | "variable"
  | "string"
  | "number"
  | "bool"
  | "null"
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
  null: Decoration.mark({ class: "mpl-null" }),
  regexp: Decoration.mark({ class: "mpl-regexp" }),
  operator: Decoration.mark({ class: "mpl-operator" }),
  punctuation: Decoration.mark({ class: "mpl-punctuation" }),
  type: Decoration.mark({ class: "mpl-type" }),
  comment: Decoration.mark({ class: "mpl-comment" }),
};

/**
 * Tokens come from the wasm tokenizer, which covers every construct including
 * comments and keywords, and yields them for a query that is still being typed.
 *
 * They arrive sorted by `from` and non-overlapping, so they can be handed to
 * `RangeSetBuilder` directly.
 */
function buildDecorations(view: EditorView): DecorationSet {
  const doc = view.state.doc.toString();

  let tokens: Token[] = [];
  try {
    tokens = (mpl.tokenize(doc) as Token[] | null) ?? [];
  } catch {
    // WASM not ready; the next update rebuilds.
    return Decoration.none;
  }

  const builder = new RangeSetBuilder<Decoration>();
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
