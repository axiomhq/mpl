import { hoverTooltip, type EditorView, type Tooltip } from "@codemirror/view";
import {
  type WasmFunctionInfo,
  formatArgType,
  getFunctionInfo,
} from "./wasm-types";
import {
  MplParamType,
  mplSystemParams,
  parseParamType,
  type MplSystemParam,
} from "./system-params";

interface KeywordDoc {
  description: string;
  syntax?: string;
}

const KEYWORD_DOCS: Record<string, KeywordDoc> = {
  filter: {
    description: "Filter time series by tag values",
    syntax: "| filter <tag> == <value>",
  },
  where: {
    description: "Filter time series by tag values (alias for filter)",
    syntax: "| where <tag> == <value>",
  },
  map: {
    description: "Apply a function to each data point",
    syntax: "| map <function>",
  },
  group: {
    description: "Group time series by tags and aggregate",
    syntax: "| group by <tags> using <function>",
  },
  align: {
    description: "Align time series to a regular time grid",
    syntax: "| align to <interval> using <function>",
  },
  bucket: {
    description: "Bucket time series into histogram buckets",
    syntax: "| bucket by <tags> to <interval> using <function>(<specs>)",
  },
  compute: {
    description: "Compute a new metric from two sources",
    syntax: "| compute <metric> using <function>",
  },
  replace: {
    description: "Replace tag values using string operations",
    syntax: "| replace <tag> ~ #s/pattern/replacement/",
  },
  join: {
    description: "Join two metric sources by tags",
    syntax: "| join <tags> from <metric_id> by <tags>",
  },
  as: { description: "Rename the output metric", syntax: "| as <name>" },
  extend: {
    description:
      "Add new constant-valued tags to every series after aggregation. Each tag must be net-new for the query — a series that already carries the tag causes the query to fail. Only constant values (strings, numbers, booleans, or scalar params) are supported.",
    syntax: "| extend <tag> = <value>, ...",
  },
  set: {
    description: "Set query directives (time range, resolution)",
    syntax: "set <directive> = <value>;",
  },
  by: { description: "Specify tags for grouping, bucketing, or joining" },
  using: { description: "Specify the function to apply" },
  to: { description: "Specify target time interval for align or bucket" },
  over: {
    description: "Specify the window duration for alignment",
    syntax: "| align to <interval> over <window> using <function>",
  },
  from: { description: "Specify the source metric for join" },
  and: { description: "Logical AND in filter expressions" },
  or: { description: "Logical OR in filter expressions" },
  not: { description: "Logical NOT in filter expressions" },
  ifdef: {
    description:
      "Conditionally apply a filter when an optional param is supplied. The body is dropped when the param is omitted; an optional `else` branch applies a different filter in that case.",
    syntax:
      "| ifdef($param) { where <filter-expr> } [else { where <else-filter-expr> }]",
  },
  else: {
    description:
      "Optional companion to `ifdef`: applies a filter when the gating optional param is *not* supplied. Only valid immediately after an `ifdef(...) { ... }` block.",
    syntax: "| ifdef($param) { ... } else { where <filter-expr> }",
  },
  Option: {
    description:
      "Wraps a param type to mark it optional. Optional params can only be referenced inside an ifdef gating on them.",
    syntax: "param $name: Option<T>;",
  },
};

/** A declared MPL parameter, as resolved from the document text. */
export interface ParamDecl {
  /**
   * Inner type as written in the source (e.g. `"string"`, `"Duration"`).
   * For optional params this is the *unwrapped* inner type: `Option<string>`
   * yields `{ type: "string", optional: true }`.
   */
  type: MplParamType;
  optional: boolean;
}

// Multi-line: matches each `param $name: type;` declaration in the document.
// Lazy on the type body to stop at the first `;` (single-line declarations).
const PARAM_LINE_RE = /^[ \t]*param[ \t]+(\$[A-Za-z_][A-Za-z0-9_]*)[ \t]*:[ \t]*([^;]+);/gm;
const OPTION_RE = /^Option[ \t]*<[ \t]*(.+?)[ \t]*>$/;

/**
 * Scans the document for `param $name: type;` declarations and returns a map
 * keyed by the dollar-prefixed name (e.g. `"$container"`).
 *
 * Intentionally TS-side rather than a wasm round-trip: the grammar for param
 * declarations is dead simple and stable, and a hover hint is non-critical
 * enough that drift risk is acceptable. If the param fails to compile, the
 * hover simply won't resolve — diagnostics handle the error path.
 */
export function parseParamDeclarations(doc: string): Map<string, ParamDecl> {
  const result = new Map<string, ParamDecl>();
  const re = new RegExp(PARAM_LINE_RE.source, PARAM_LINE_RE.flags);
  let m: RegExpExecArray | null;
  while ((m = re.exec(doc)) !== null) {
    const [, name, rawType] = m;
    const trimmed = rawType.trim();
    const optMatch = OPTION_RE.exec(trimmed);
    const type = parseParamType(optMatch ? optMatch[1] : trimmed);
    if (type === undefined) continue; // unknown type — diagnostics handle the error path
    result.set(name, { type, optional: optMatch !== null });
  }
  return result;
}

/**
 * Locates a `$ident` token at `pos`. The cursor may sit on the `$` itself or
 * on any character of the name. Returns the dollar-prefixed name and the
 * span covering it, or `null` when there is no param reference at the cursor.
 */
export function extractParamAt(
  doc: string,
  pos: number,
): { name: string; from: number; to: number } | null {
  if (pos < 0 || pos >= doc.length) return null;
  const isIdChar = (c: string) => /[A-Za-z0-9_]/.test(c);

  let from = pos;
  if (doc[from] !== "$") {
    if (!isIdChar(doc[from])) return null;
    while (from > 0 && isIdChar(doc[from - 1])) from--;
    if (from === 0 || doc[from - 1] !== "$") return null;
    from--;
  }

  let to = from + 1;
  while (to < doc.length && isIdChar(doc[to])) to++;

  if (to === from + 1) return null; // bare `$` without an identifier
  return { name: doc.slice(from, to), from, to };
}

function extractWordAt(
  doc: string,
  pos: number,
): { text: string; from: number; to: number } | null {
  if (pos < 0 || pos >= doc.length) return null;

  const isIdChar = (i: number) =>
    i >= 0 && i < doc.length && /[\w]/.test(doc[i]);

  if (!isIdChar(pos)) return null;

  let from = pos;
  let to = pos + 1;

  while (from > 0 && isIdChar(from - 1)) from--;
  while (to < doc.length && isIdChar(to)) to++;

  // Extend left across :: separators for qualified names (e.g. prom::rate)
  while (from >= 2 && doc[from - 1] === ":" && doc[from - 2] === ":") {
    let newFrom = from - 2;
    if (newFrom > 0 && isIdChar(newFrom - 1)) {
      newFrom--;
      while (newFrom > 0 && isIdChar(newFrom - 1)) newFrom--;
      from = newFrom;
    } else {
      break;
    }
  }

  // Extend right across :: separators
  while (to + 1 < doc.length && doc[to] === ":" && doc[to + 1] === ":") {
    const newTo = to + 2;
    if (newTo < doc.length && isIdChar(newTo)) {
      to = newTo + 1;
      while (to < doc.length && isIdChar(to)) to++;
    } else {
      break;
    }
  }

  const text = doc.slice(from, to);
  if (text.length === 0 || !/[a-zA-Z]/.test(text)) return null;

  return { text, from, to };
}

function renderFunctionTooltip(info: WasmFunctionInfo): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "mpl-hover-tooltip";

  const sig = document.createElement("div");
  sig.className = "mpl-hover-sig";

  const fnName = document.createElement("span");
  fnName.className = "mpl-hover-fn";
  fnName.textContent = info.label;
  sig.appendChild(fnName);

  if (info.args.length > 0) {
    sig.appendChild(document.createTextNode("("));
    info.args.forEach((arg, i) => {
      if (i > 0) sig.appendChild(document.createTextNode(", "));
      const span = document.createElement("span");
      span.className = "mpl-hover-param";
      span.textContent = `${arg.name}: ${formatArgType(arg.type)}`;
      sig.appendChild(span);
    });
    sig.appendChild(document.createTextNode(")"));
  }

  dom.appendChild(sig);

  if (info.info) {
    const docEl = document.createElement("div");
    docEl.className = "mpl-hover-doc";
    docEl.textContent = info.info;
    dom.appendChild(docEl);
  }

  return dom;
}

function renderParamTooltip(name: string, decl: ParamDecl): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "mpl-hover-tooltip";

  const sig = document.createElement("div");
  sig.className = "mpl-hover-sig";

  const nameSpan = document.createElement("span");
  nameSpan.className = "mpl-hover-fn";
  nameSpan.textContent = name;
  sig.appendChild(nameSpan);

  sig.appendChild(document.createTextNode(": "));

  const typeSpan = document.createElement("span");
  typeSpan.className = "mpl-hover-param";
  typeSpan.textContent = decl.optional ? `Option<${decl.type}>` : decl.type;
  sig.appendChild(typeSpan);

  dom.appendChild(sig);

  if (decl.optional) {
    const note = document.createElement("div");
    note.className = "mpl-hover-doc";
    note.textContent =
      "Optional parameter — only referenceable inside an `ifdef` block gating on it.";
    dom.appendChild(note);
  }

  return dom;
}

function renderKeywordTooltip(keyword: string, doc: KeywordDoc): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "mpl-hover-tooltip";

  const header = document.createElement("div");
  const kw = document.createElement("span");
  kw.className = "mpl-hover-keyword";
  kw.textContent = keyword;
  header.appendChild(kw);
  dom.appendChild(header);

  const desc = document.createElement("div");
  desc.className = "mpl-hover-doc";
  desc.textContent = doc.description;
  dom.appendChild(desc);

  if (doc.syntax) {
    const syntax = document.createElement("div");
    syntax.className = "mpl-hover-syntax";
    syntax.textContent = doc.syntax;
    dom.appendChild(syntax);
  }

  return dom;
}

function hoverSource(
  view: EditorView,
  pos: number,
  _side: -1 | 1,
): Tooltip | null {
  const doc = view.state.doc.toString();

  // Param references take priority over the generic word path. `$ident`
  // tokens are not picked up by `extractWordAt` (the leading `$` short-
  // circuits its `[a-zA-Z]` guard at the end), so without this branch
  // hovering a `$container` reference would render no tooltip at all.
  const param = extractParamAt(doc, pos);
  if (param) {
    // Inline declarations override host-supplied system params on name
    // collision — same precedence the completion source enforces.
    const decls = parseParamDeclarations(doc);
    const systemParams = view.state.facet(mplSystemParams);
    mergeSystemParamsInto(decls, systemParams);
    const decl = decls.get(param.name);
    if (decl) {
      return {
        pos: param.from,
        end: param.to,
        above: true,
        create() {
          return { dom: renderParamTooltip(param.name, decl) };
        },
      };
    }
    // Param referenced but not declared — let diagnostics flag the
    // undefined name; suppress the hover instead of showing a stale or
    // misleading tooltip.
    return null;
  }

  const word = extractWordAt(doc, pos);
  if (!word) return null;

  const fnInfo = getFunctionInfo(word.text);
  if (fnInfo) {
    return {
      pos: word.from,
      end: word.to,
      above: true,
      create() {
        return { dom: renderFunctionTooltip(fnInfo) };
      },
    };
  }

  const kwDoc = KEYWORD_DOCS[word.text];
  if (kwDoc) {
    return {
      pos: word.from,
      end: word.to,
      above: true,
      create() {
        return { dom: renderKeywordTooltip(word.text, kwDoc) };
      },
    };
  }

  return null;
}

export const mplHover = hoverTooltip(hoverSource, { hideOnChange: true });

/**
 * Splices host-supplied system params into a declaration map produced by
 * `parseParamDeclarations`, without overwriting inline declarations that
 * share a name. Names supplied without the leading `$` are normalised so
 * the map key matches what `extractParamAt` returns from the document.
 *
 * Exported for unit tests; production consumers go through `mplHover` and
 * the `mplSystemParams` facet.
 */
export function mergeSystemParamsInto(
  decls: Map<string, ParamDecl>,
  systemParams: readonly MplSystemParam[],
): void {
  for (const sp of systemParams) {
    const key = sp.name.startsWith("$") ? sp.name : `$${sp.name}`;
    if (decls.has(key)) continue;
    decls.set(key, { type: sp.type, optional: sp.optional ?? false });
  }
}
