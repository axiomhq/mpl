import { showTooltip, type Tooltip } from "@codemirror/view";
import { type EditorState } from "@codemirror/state";
import { type WasmFunctionInfo, formatArgType, getFunctionInfo } from "./wasm-types";

interface CallContext {
  fnLabel: string;
  openParenPos: number;
  activeParam: number;
}

function findCallContext(doc: string, cursor: number): CallContext | null {
  // Scan left from cursor to find the nearest unmatched '('
  let depth = 0;
  let openParenPos = -1;
  let i = cursor - 1;

  while (i >= 0) {
    const ch = doc[i];

    // Skip string literals (scan backwards past closing quote)
    if (ch === '"') {
      i--;
      while (i >= 0) {
        if (doc[i] === '"') {
          // Count consecutive backslashes before this quote
          let bs = 0;
          let b = i - 1;
          while (b >= 0 && doc[b] === '\\') { bs++; b--; }
          if (bs % 2 === 0) break; // unescaped opening quote
        }
        i--;
      }
      i--;
      continue;
    }

    // Skip regex literals: #/pattern/ or #s/src/dst/
    // When we hit a closing '/', scan backwards past the regex body to '#'
    if (ch === '/') {
      const saved = i;
      let slashCount = 1;
      i--;
      while (i >= 0) {
        if (doc[i] === '/' && (i === 0 || doc[i - 1] !== '\\')) {
          slashCount++;
          if (i > 0 && doc[i - 1] === '#') {
            // #/pattern/
            i -= 2;
            break;
          }
          if (i > 1 && doc[i - 1] === 's' && doc[i - 2] === '#') {
            // #s/src/dst/
            i -= 3;
            break;
          }
        }
        i--;
      }
      // If we didn't find a valid regex opener, restore position
      if (i < -1 || (i >= 0 && slashCount < 2)) {
        i = saved - 1;
      }
      continue;
    }

    if (ch === ')') {
      depth++;
    } else if (ch === '(') {
      if (depth === 0) {
        openParenPos = i;
        break;
      }
      depth--;
    }
    i--;
  }

  if (openParenPos < 0) return null;

  // Extract function label before the '('
  let labelEnd = openParenPos;
  // Skip whitespace before '('
  let j = labelEnd - 1;
  while (j >= 0 && (doc[j] === ' ' || doc[j] === '\t')) j--;

  if (j < 0) return null;

  // Scan backwards to collect the function identifier (alphanumeric, _, :, .)
  const labelEndPos = j + 1;
  while (j >= 0 && /[\w:.]/.test(doc[j])) j--;
  const labelStart = j + 1;

  if (labelStart >= labelEndPos) return null;

  const fnLabel = doc.slice(labelStart, labelEndPos);
  // Reject labels that are just operators or end with ::
  if (fnLabel.endsWith("::") || !/[a-zA-Z]/.test(fnLabel)) return null;

  // Compute active parameter index: count commas at depth 0 between '(' and cursor
  let commaCount = 0;
  let innerDepth = 0;
  for (let k = openParenPos + 1; k < cursor; k++) {
    const c = doc[k];
    if (c === '(') innerDepth++;
    else if (c === ')') innerDepth--;
    else if (c === ',' && innerDepth === 0) commaCount++;
    else if (c === '"') {
      k++;
      while (k < cursor && doc[k] !== '"') {
        if (doc[k] === '\\') k++;
        k++;
      }
    } else if (c === '#' && k + 1 < cursor && doc[k + 1] === '/') {
      k += 2;
      while (k < cursor && doc[k] !== '/') {
        if (doc[k] === '\\') k++;
        k++;
      }
    } else if (c === '#' && k + 2 < cursor && doc[k + 1] === 's' && doc[k + 2] === '/') {
      k += 3;
      for (let slashes = 0; k < cursor && slashes < 2;) {
        if (doc[k] === '\\') { k++; }
        else if (doc[k] === '/') { slashes++; }
        k++;
      }
      k--;
    }
  }

  return { fnLabel, openParenPos, activeParam: commaCount };
}

function renderSignatureTooltip(info: WasmFunctionInfo, activeParam: number): HTMLElement {
  const dom = document.createElement("div");
  dom.className = "mpl-signature-help";

  const sig = document.createElement("div");
  sig.className = "mpl-signature-sig";

  const fnName = document.createElement("span");
  fnName.className = "mpl-signature-fn";
  fnName.textContent = info.label;
  sig.appendChild(fnName);
  sig.appendChild(document.createTextNode("("));

  info.args.forEach((arg, i) => {
    if (i > 0) sig.appendChild(document.createTextNode(", "));
    const span = document.createElement("span");
    span.className = i === activeParam ? "mpl-signature-param active" : "mpl-signature-param";
    span.textContent = `${arg.name}: ${formatArgType(arg.type)}`;
    sig.appendChild(span);
  });

  sig.appendChild(document.createTextNode(")"));
  dom.appendChild(sig);

  if (info.info) {
    const doc = document.createElement("div");
    doc.className = "mpl-signature-doc";
    doc.textContent = info.info;
    dom.appendChild(doc);
  }

  return dom;
}

function getSignatureTooltip(state: EditorState): Tooltip | null {
  const cursor = state.selection.main.head;
  const doc = state.doc.toString();

  const ctx = findCallContext(doc, cursor);
  if (!ctx) return null;

  const info = getFunctionInfo(ctx.fnLabel);
  if (!info || info.args.length === 0) return null;

  return {
    pos: ctx.openParenPos,
    above: true,
    arrow: true,
    create() {
      return { dom: renderSignatureTooltip(info, ctx.activeParam) };
    },
  };
}

export const mplSignatureHelp = showTooltip.compute(
  ["doc", "selection"],
  getSignatureTooltip,
);
