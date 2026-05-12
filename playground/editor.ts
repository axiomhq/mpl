// CodeMirror editor setup

import type { MplCompletionConfig, MplSystemParam } from "@axiomhq/mpl-codemirror";
import {
  createMplCompletion,
  mplHighlighter,
  mplHover,
  mplLinter,
  mplSignatureHelp,
  mplSystemParams,
} from "@axiomhq/mpl-codemirror";
import { Compartment, EditorState } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { vim } from "@replit/codemirror-vim";
import { datasets } from "./datasets";

/**
 * Host-supplied parameters the query service injects at execution time.
 *
 * Each entry serves two roles:
 *  - `{ name, type }` feeds the `mplSystemParams` facet so the language
 *    server stops flagging `$__interval` as undeclared.
 *  - `value` is the concrete string the playground splices in for the
 *    name before handing the query to the interpreter, which has no
 *    binding step of its own (`Parameterized::Param` would otherwise
 *    fail with “Parameterized values are not supported” at runtime).
 *
 * System param names must use the `__` prefix — the parser surfaces a
 * diagnostic for any registration that doesn't.
 */
interface PlaygroundSystemParam extends MplSystemParam {
  /** Concrete value substituted into the query text before interpretation. */
  value: string;
}

const SYSTEM_PARAMS: PlaygroundSystemParam[] = [
  // 1 minute matches a reasonable default resolution for the demo datasets.
  // Hosts in production compute this from the query window; the playground
  // pins it for reproducibility.
  { name: "__interval", type: "Duration", value: "1m" },
];

export const SYSTEM_PARAM_FACET: MplSystemParam[] = SYSTEM_PARAMS.map(
  ({ name, type, optional }) =>
    optional === undefined ? { name, type } : { name, type, optional },
);

/**
 * Substitutes every registered system-param reference with its concrete
 * value, returning a query string the playground interpreter can run
 * without a binding step.
 *
 * Word-boundary matched on the right so `$__interval_extended` is left
 * untouched. Naive in that it does not skip occurrences inside string or
 * regex literals — system params aren't meaningful in those positions,
 * and a parser-aware substituter would be a significant new surface for
 * little gain.
 */
export function substituteSystemParams(doc: string): string {
  let out = doc;
  for (const { name, value } of SYSTEM_PARAMS) {
    // Escape regex metacharacters in the name defensively, even though the
    // current set is alphanumeric + underscore.
    const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    out = out.replace(new RegExp(`\\$${escaped}\\b`, "g"), value);
  }
  return out;
}

const completionConfig: MplCompletionConfig = {
  datasets: async () => datasets.map((ds) => ds.name),
  metrics: async (dataset: string) =>
    datasets.find((ds) => ds.name === dataset)?.metrics.map((m) => m.name) ?? [],
  tags: async (dataset: string, metric: string) => {
    const series =
      datasets.find((ds) => ds.name === dataset)?.metrics.find((m) => m.name === metric)?.series ??
      [];
    const tags = new Set<string>();
    for (const s of series) {
      for (const k of Object.keys(s.tags)) tags.add(k);
    }
    return [...tags].sort();
  },
};

export interface EditorInstance {
  view: EditorView;
  setVimMode(enabled: boolean): void;
  setTheme(resolved: "dark" | "light"): void;
}

export function createEditor(
  parent: HTMLElement,
  initialTheme: "dark" | "light",
  initialVim: boolean,
  onChange: () => void,
): EditorInstance {
  const vimCompartment = new Compartment();
  const themeCompartment = new Compartment();

  const view = new EditorView({
    doc: "",
    extensions: [
      basicSetup,
      EditorState.allowMultipleSelections.of(true),
      vimCompartment.of(initialVim ? vim() : []),
      themeCompartment.of(initialTheme === "dark" ? oneDark : []),
      EditorView.lineWrapping,
      mplHighlighter,
      mplSystemParams.of(SYSTEM_PARAM_FACET),
      createMplCompletion(completionConfig),
      mplLinter,
      mplSignatureHelp,
      mplHover,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) onChange();
      }),
    ],
    parent,
  });

  return {
    view,
    setVimMode(enabled: boolean) {
      view.dispatch({ effects: vimCompartment.reconfigure(enabled ? vim() : []) });
    },
    setTheme(resolved: "dark" | "light") {
      view.dispatch({ effects: themeCompartment.reconfigure(resolved === "dark" ? oneDark : []) });
    },
  };
}
