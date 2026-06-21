import { ViewPlugin, ViewUpdate, Decoration, DecorationSet, EditorView } from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";
import * as mpl from "@axiomhq/mpl";

// Token types mirror `mpl_lang::slice::HlKind` (see src/slice.rs), surfaced
// through `mpl-language-server`'s `TokenType`.
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

// All highlighting now comes from the Rust `chumsky` lexer via WASM.
//
// Previously this file re-implemented the MPL grammar as a pile of JS regexes
// (keywords, strings, regex literals, numbers, booleans, types, comments) and
// used them as a fallback whenever the pest parser failed — which was *every*
// incomplete/mid-edit query. The chumsky lexer is total: it returns spans for
// any input, so the fallback (and the grammar duplication it carried) is gone.
function buildDecorations(view: EditorView): DecorationSet {
  const doc = view.state.doc.toString();

  let tokens: Token[] = [];
  try {
    tokens = (mpl.tokenize(doc) as Token[] | null) ?? [];
  } catch {
    // WASM not ready yet — render unstyled rather than crash.
    return Decoration.none;
  }

  // The lexer already emits sorted, non-overlapping spans; sort defensively
  // because RangeSetBuilder requires ascending order.
  tokens.sort((a, b) => a.from - b.from);

  const builder = new RangeSetBuilder<Decoration>();
  for (const t of tokens) {
    const d = decos[t.type];
    if (d && t.from < t.to) {
      builder.add(t.from, t.to, d);
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
