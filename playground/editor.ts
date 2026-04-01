// CodeMirror editor setup

import type { MplCompletionConfig } from "@axiomhq/mpl-codemirror";
import {
  createMplCompletion,
  mplHighlighter,
  mplHover,
  mplLinter,
  mplSignatureHelp,
} from "@axiomhq/mpl-codemirror";
import { Compartment, EditorState } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { EditorView } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { vim } from "@replit/codemirror-vim";
import { loadDatasetIndex } from "./datasets";

const completionConfig: MplCompletionConfig = {
  datasets: async () => Object.keys(await loadDatasetIndex()),
  metrics: async (dataset: string) => Object.keys((await loadDatasetIndex())[dataset] ?? {}),
  tags: async (dataset: string, metric: string) =>
    (await loadDatasetIndex())[dataset]?.[metric] ?? [],
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
