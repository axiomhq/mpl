import { hoverTooltip, type EditorView, type Tooltip } from "@codemirror/view";
import {
  type WasmFunctionInfo,
  formatArgType,
  getFunctionInfo,
} from "./wasm-types";

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
};

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
  _view: EditorView,
  pos: number,
  _side: -1 | 1,
): Tooltip | null {
  const doc = _view.state.doc.toString();
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
