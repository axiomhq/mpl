import { ViewPlugin, ViewUpdate, Decoration, DecorationSet, EditorView } from "@codemirror/view";
import { RangeSetBuilder } from "@codemirror/state";
import * as mpl from "@axiomhq/mpl-lang";

type TokenType = "variable" | "string" | "number" | "bool" | "regexp" | "operator" | "punctuation" | "keyword" | "type";

interface Token {
  from: number;
  to: number;
  type: TokenType;
}

const MPL_KEYWORDS = /\b(filter|where|map|group|by|using|align|to|over|from|bucket|join|compute|set|replace|as|and|or|not|is|param|rate|increase|histogram|interpolate_delta_histogram|interpolate_cumulative_histogram|count|avg|sum|min|max)\b/g;
const COMMENT_RE = /\/\/[^\n]*/g;
const STRING_RE = /"(?:[^"\\]|\\.)*"/g;
const REGEX_RE = /#s?\/(?:[^/\\]|\\.)*(?:\/(?:[^/\\]|\\.)*)?\//g;
const NUMBER_RE = /\b\d+(\.\d*)?([eE][+-]?\d+)?[smhdwMy]?\b/g;
const BOOL_RE = /\b(true|false)\b/g;
const TYPE_RE = /\b(string|int|float|bool|dataset|metric|duration|regex)\b/g;

const decos: Record<string, Decoration> = {
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

interface TokenEntry {
  from: number;
  to: number;
  deco: Decoration;
  priority: number;
}

function findMatches(
  re: RegExp,
  doc: string,
  deco: Decoration,
  priority: number,
  out: TokenEntry[],
) {
  re.lastIndex = 0;
  let m;
  while ((m = re.exec(doc)) !== null) {
    out.push({ from: m.index, to: m.index + m[0].length, deco, priority });
  }
}

function buildDecorations(view: EditorView): DecorationSet {
  const doc = view.state.doc.toString();
  const entries: TokenEntry[] = [];

  // Comments always via regex (pest COMMENT rule is silent)
  findMatches(COMMENT_RE, doc, decos.comment, 30, entries);

  let wasmTokens: Token[] | null = null;
  try {
    wasmTokens = mpl.tokenize(doc) as Token[] | null;
  } catch {
    // WASM not ready or parse failed
  }

  if (wasmTokens && wasmTokens.length > 0) {
    // Add WASM tokens
    for (const t of wasmTokens) {
      const d = decos[t.type];
      if (d) entries.push({ from: t.from, to: t.to, deco: d, priority: 20 });
    }

    // Find keywords in gaps between WASM tokens
    const occupied = wasmTokens.map((t) => [t.from, t.to] as [number, number]);
    findKeywordsInGaps(doc, occupied, entries);
  } else {
    // Regex fallback for when WASM parse fails (incomplete input)
    findMatches(STRING_RE, doc, decos.string, 10, entries);
    findMatches(REGEX_RE, doc, decos.regexp, 10, entries);
    findMatches(BOOL_RE, doc, decos.bool, 10, entries);
    findMatches(NUMBER_RE, doc, decos.number, 5, entries);
    findMatches(TYPE_RE, doc, decos.type, 10, entries);
    findMatches(MPL_KEYWORDS, doc, decos.keyword, 10, entries);
  }

  return resolveAndBuild(entries);
}

function findKeywordsInGaps(
  doc: string,
  occupied: [number, number][],
  entries: TokenEntry[],
) {
  MPL_KEYWORDS.lastIndex = 0;
  let m;
  while ((m = MPL_KEYWORDS.exec(doc)) !== null) {
    const from = m.index;
    const to = from + m[0].length;
    if (!occupied.some(([s, e]) => from < e && to > s)) {
      entries.push({ from, to, deco: decos.keyword, priority: 10 });
    }
  }
}

function resolveAndBuild(entries: TokenEntry[]): DecorationSet {
  // Sort by priority descending, then position
  entries.sort((a, b) => b.priority - a.priority || a.from - b.from);

  // Greedily select non-overlapping tokens (higher priority wins)
  const selected: TokenEntry[] = [];
  for (const e of entries) {
    if (!selected.some((s) => e.from < s.to && e.to > s.from)) {
      selected.push(e);
    }
  }

  // RangeSetBuilder requires ascending order
  selected.sort((a, b) => a.from - b.from);

  const builder = new RangeSetBuilder<Decoration>();
  for (const e of selected) {
    if (e.from < e.to) {
      builder.add(e.from, e.to, e.deco);
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
