import { linter, type Diagnostic, type Action } from "@codemirror/lint";
import { type EditorView } from "@codemirror/view";
import * as mpl from "@axiomhq/mpl";
import { mplSystemParams } from "./system-params";

export type WasmDiagnosticSeverity = "error" | "warning" | "info" | "hint";

export interface WasmDiagnosticAction {
  name: string;
  from: number;
  to: number;
  insert: string;
}

export interface WasmDiagnosticItem {
  from: number;
  to: number;
  severity: WasmDiagnosticSeverity;
  message: string;
  help?: string;
  actions?: WasmDiagnosticAction[];
}

function mapActions(wasmActions?: WasmDiagnosticAction[]): Action[] | undefined {
  if (!wasmActions || wasmActions.length === 0) return undefined;
  return wasmActions.map(a => ({
    name: a.name,
    apply(view: EditorView) {
      view.dispatch({ changes: { from: a.from, to: a.to, insert: a.insert } });
    },
  }));
}

/**
 * Pure mapping from wasm diagnostic items to CodeMirror `Diagnostic`s.
 *
 * Exported so callers (and unit tests) can exercise the translation
 * without instantiating an `EditorView` or loading the WASM module.
 * `to` is forced to at least `from + 1` because CodeMirror collapses
 * zero-width diagnostics; the wasm side legitimately emits zero-width
 * spans (e.g. "expected metric name" at EOF) and we want them visible.
 */
export function mapDiagnostics(items: WasmDiagnosticItem[]): Diagnostic[] {
  return items.map(item => ({
    from: item.from,
    to: Math.max(item.from + 1, item.to),
    severity: item.severity,
    message: item.help ? `${item.message}\n${item.help}` : item.message,
    actions: mapActions(item.actions),
  }));
}

/**
 * Lint source backing `mplLinter`. Exported for testability — production
 * consumers should use `mplLinter`, which is the `linter()`-wrapped
 * extension that CodeMirror schedules.
 */
export function mplLintSource(view: EditorView): Diagnostic[] {
  const doc = view.state.doc.toString();
  // Host-supplied system params (e.g. `$__interval`) live in this facet.
  // Defaults to `[]` when no provider is present, matching pre-feature
  // behaviour. The wasm bridge accepts null/undefined/missing as "none".
  const systemParams = view.state.facet(mplSystemParams);

  let items: WasmDiagnosticItem[];
  try {
    items = (mpl.diagnostics(doc, systemParams) as WasmDiagnosticItem[] | null) ?? [];
  } catch {
    return [];
  }

  return mapDiagnostics(items);
}

export const mplLinter = linter(mplLintSource);
