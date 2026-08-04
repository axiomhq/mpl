import { hoverTooltip, type EditorView, type Tooltip } from "@codemirror/view";
import * as mpl from "@axiomhq/mpl";
import {
  type WasmFunctionInfo,
  formatArgType,
  getFunctionInfo,
} from "./wasm-types";
import { mplSystemParams, type MplSystemParam } from "./system-params";

/** One entry of the `declared_params` wasm result. */
interface WasmDeclaredParam {
  name: string;
  type: string;
  optional: boolean;
}

/** The `token_at` wasm result: a span plus what the token is. */
interface WasmToken {
  from: number;
  to: number;
  type: string;
}

/** The `keyword_info` wasm result. */
interface WasmKeywordInfo {
  label: string;
  description: string;
  syntax?: string;
}

function keywordInfo(label: string): WasmKeywordInfo | null {
  try {
    return (mpl.keyword_info(label) as WasmKeywordInfo | null) ?? null;
  } catch {
    return null;
  }
}

/** A declared MPL parameter, as resolved from the document text. */
export interface ParamDecl {
  /**
   * Inner type as written in the source (e.g. `"string"`, `"Duration"`).
   * For optional params this is the *unwrapped* inner type: `Option<string>`
   * yields `{ type: "string", optional: true }`.
   */
  type: string;
  optional: boolean;
}

/**
 * Reads the `param $name: type;` declarations out of the document, keyed by the
 * dollar-prefixed name (e.g. `"$container"`).
 *
 * Delegates to the language server rather than re-deriving the declaration
 * grammar here, so hover, completion and diagnostics cannot disagree about what
 * a query declares. Tolerates an unfinished query body.
 */
export function parseParamDeclarations(doc: string): Map<string, ParamDecl> {
  const result = new Map<string, ParamDecl>();
  let declared: WasmDeclaredParam[] | null = null;
  try {
    declared = (mpl.declared_params(doc) as WasmDeclaredParam[] | null) ?? null;
  } catch {
    // WASM not ready; no declarations to report yet.
  }
  for (const p of declared ?? []) {
    result.set(`$${p.name}`, { type: p.type, optional: p.optional });
  }
  return result;
}

/**
 * The token at `pos`, as the language server's lexer sees it. A `::`-qualified
 * function name comes back whole, which is the name the stdlib is keyed by.
 */
function tokenAt(doc: string, pos: number): WasmToken | null {
  try {
    return (mpl.token_at(doc, pos) as WasmToken | null) ?? null;
  } catch {
    // WASM not ready.
    return null;
  }
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

function renderKeywordTooltip(doc: WasmKeywordInfo): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "mpl-hover-tooltip";

  const header = document.createElement("div");
  const kw = document.createElement("span");
  kw.className = "mpl-hover-keyword";
  kw.textContent = doc.label;
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

  const token = tokenAt(doc, pos);
  if (!token) return null;
  const text = doc.slice(token.from, token.to);

  // Param references take priority over the generic word path. Both lex as
  // variables; the leading `$` is what tells a param from a tag.
  if (text.startsWith("$")) {
    // Inline declarations override host-supplied system params on name
    // collision — same precedence the completion source enforces.
    const decls = parseParamDeclarations(doc);
    const systemParams = view.state.facet(mplSystemParams);
    mergeSystemParamsInto(decls, systemParams);
    const decl = decls.get(text);
    if (decl) {
      return {
        pos: token.from,
        end: token.to,
        above: true,
        create() {
          return { dom: renderParamTooltip(text, decl) };
        },
      };
    }
    // Param referenced but not declared — let diagnostics flag the
    // undefined name; suppress the hover instead of showing a stale or
    // misleading tooltip.
    return null;
  }

  const fnInfo = getFunctionInfo(text);
  if (fnInfo) {
    return {
      pos: token.from,
      end: token.to,
      above: true,
      create() {
        return { dom: renderFunctionTooltip(fnInfo) };
      },
    };
  }

  const kwDoc = keywordInfo(text);
  if (kwDoc) {
    return {
      pos: token.from,
      end: token.to,
      above: true,
      create() {
        return { dom: renderKeywordTooltip(kwDoc) };
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
 * the map key matches the `$name` form the token lookup returns.
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
