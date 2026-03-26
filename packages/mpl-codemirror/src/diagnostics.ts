import { linter, type Diagnostic, type Action } from "@codemirror/lint";
import { type EditorView } from "@codemirror/view";
import * as mpl from "@axiomhq/mpl";

type Severity = "error" | "warning" | "info" | "hint";

interface WasmDiagnosticAction {
  name: string;
  from: number;
  to: number;
  insert: string;
}

interface WasmDiagnosticItem {
  from: number;
  to: number;
  severity: Severity;
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

function mplLintSource(view: EditorView): Diagnostic[] {
  const doc = view.state.doc.toString();

  let items: WasmDiagnosticItem[];
  try {
    items = (mpl.diagnostics(doc) as WasmDiagnosticItem[] | null) ?? [];
  } catch {
    return [];
  }

  return items.map(item => ({
    from: item.from,
    to: Math.max(item.from + 1, item.to),
    severity: item.severity,
    message: item.help ? `${item.message}\n${item.help}` : item.message,
    actions: mapActions(item.actions),
  }));
}

export const mplLinter = linter(mplLintSource);
