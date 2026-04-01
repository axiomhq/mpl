// CodeMirror editor setup

import type { MplCompletionConfig } from "@axiomhq/mpl-codemirror";
import {
  createMplCompletion,
  mplHighlighter,
  mplHover,
  mplLinter,
  mplSignatureHelp,
} from "@axiomhq/mpl-codemirror";
import {
  autocompletion,
  closeBrackets,
  closeBracketsKeymap,
  completionKeymap,
} from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import {
  bracketMatching,
  defaultHighlightStyle,
  foldGutter,
  foldKeymap,
  indentOnInput,
  syntaxHighlighting,
} from "@codemirror/language";
import { lintKeymap } from "@codemirror/lint";
import { highlightSelectionMatches, searchKeymap } from "@codemirror/search";
import { Compartment, EditorState } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import {
  crosshairCursor,
  drawSelection,
  dropCursor,
  EditorView,
  highlightActiveLine,
  highlightActiveLineGutter,
  highlightSpecialChars,
  keymap,
  rectangularSelection,
} from "@codemirror/view";
import { vim } from "@replit/codemirror-vim";
import { loadDatasetIndex } from "./datasets";
import { resolveTheme, type Theme } from "./theme";

const completionConfig: MplCompletionConfig = {
  datasets: async () => Object.keys(await loadDatasetIndex()),
  metrics: async (dataset: string) => Object.keys((await loadDatasetIndex())[dataset] ?? {}),
  tags: async (dataset: string, metric: string) =>
    (await loadDatasetIndex())[dataset]?.[metric] ?? [],
};

const mplCompletion = createMplCompletion(completionConfig);

export interface EditorInstance {
  view: EditorView;
  setVimMode(enabled: boolean): void;
  setTheme(theme: Theme): void;
}

export function createEditor(
  parent: HTMLElement,
  initialTheme: Theme,
  initialVim: boolean,
  onChange: () => void,
): EditorInstance {
  const vimCompartment = new Compartment();
  const themeCompartment = new Compartment();
  const completionCompartment = new Compartment();
  const diagnosticsCompartment = new Compartment();
  const signatureCompartment = new Compartment();
  const hoverCompartment = new Compartment();

  const getVim = (enabled: boolean) => (enabled ? vim() : []);
  const getTheme = (theme: Theme) => (resolveTheme(theme) === "dark" ? oneDark : []);

  const view = new EditorView({
    doc: "",
    extensions: [
      highlightActiveLineGutter(),
      highlightSpecialChars(),
      history(),
      foldGutter(),
      drawSelection(),
      dropCursor(),
      EditorState.allowMultipleSelections.of(true),
      indentOnInput(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
      bracketMatching(),
      closeBrackets(),
      autocompletion(),
      rectangularSelection(),
      crosshairCursor(),
      highlightActiveLine(),
      highlightSelectionMatches(),
      keymap.of([
        ...closeBracketsKeymap,
        ...defaultKeymap,
        ...searchKeymap,
        ...historyKeymap,
        ...foldKeymap,
        ...completionKeymap,
        ...lintKeymap,
      ]),
      vimCompartment.of(getVim(initialVim)),
      themeCompartment.of(getTheme(initialTheme)),
      EditorView.lineWrapping,
      mplHighlighter,
      completionCompartment.of(mplCompletion),
      diagnosticsCompartment.of(mplLinter),
      signatureCompartment.of(mplSignatureHelp),
      hoverCompartment.of(mplHover),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) onChange();
      }),
    ],
    parent,
  });

  return {
    view,
    setVimMode(enabled: boolean) {
      view.dispatch({ effects: vimCompartment.reconfigure(getVim(enabled)) });
    },
    setTheme(theme: Theme) {
      view.dispatch({ effects: themeCompartment.reconfigure(getTheme(theme)) });
    },
  };
}
