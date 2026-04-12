// Alpine.js store — glues editor, pipeline, and UI together.

import wasmInitLang from "@axiomhq/mpl-lang";
import wasmInitPlayground, { Interpreter, RunOutput } from "@axiomhq/mpl-playground";
import Alpine from "alpinejs";
import { renderCharts, type ChartEntry } from "./charts";
import { datasets } from "./datasets";
import { createEditor, type EditorInstance } from "./editor";

const exampleModules = import.meta.glob("./examples/*.mpl", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

const examples = Object.entries(exampleModules)
  .map(([path, content]) => ({
    name: path.split("/").pop()!,
    content,
  }))
  .sort((a, b) => a.name.localeCompare(b.name));

function loadSelectedExample(index: number) {
  const example = examples[index];
  if (!example) return;

  editor.view.dispatch({
    changes: { from: 0, to: editor.view.state.doc.length, insert: example.content },
  });
  editor.view.focus();
}

let editor: EditorInstance;

type Theme = "light" | "dark" | "system";

function resolveTheme(theme: Theme): "dark" | "light" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return theme;
}

await Promise.all([wasmInitLang(), wasmInitPlayground()]);
const interpreter = new Interpreter(datasets);

function onEditorChange() {
  const panel = document.getElementById("charts-panel");
  if (!panel) return;

  const doc = editor.view.state.doc.toString();
  let steps: RunOutput;

  try {
    steps = interpreter.run(doc);
  } catch (e) {
    renderCharts(panel, [{ label: doc.trim(), series: [], error: String(e) }]);
    return;
  }

  const entries: ChartEntry[] = steps.map((step) => {
    if ("Err" in step.result) return { label: step.text, series: [], error: step.result.Err };
    return { label: step.text, series: step.result.Ok };
  });

  renderCharts(panel, entries);
}

const playgroundStore = {
  editorMode: (localStorage.getItem("mpl-editorMode") as "vim" | "normie") || "vim",
  theme: (localStorage.getItem("mpl-theme") as Theme) || "system",
  resolvedTheme: "light" as "dark" | "light",
  selectedExampleIndex: 0,
  examples,

  init() {
    this.resolvedTheme = resolveTheme(this.theme);
    if (this.resolvedTheme === "dark") {
      document.documentElement.classList.add("dark-theme");
    }

    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
      if (this.theme === "system") this.onThemeChange();
    });

    this.setupDivider();
  },

  setupDivider() {
    const divider = document.getElementById("divider");
    const container = document.getElementById("container");
    const editorWrapper = container?.querySelector(".editor-wrapper") as HTMLElement | null;
    if (!divider || !container || !editorWrapper) return;

    let dragging = false;

    const isVertical = () => window.matchMedia("(max-width: 768px)").matches;

    divider.addEventListener("mousedown", (e) => {
      e.preventDefault();
      dragging = true;
      divider.classList.add("dragging");
      document.body.style.cursor = isVertical() ? "row-resize" : "col-resize";
      document.body.style.userSelect = "none";
    });

    document.addEventListener("mousemove", (e) => {
      if (!dragging) return;
      const rect = container.getBoundingClientRect();
      if (isVertical()) {
        const pct = ((e.clientY - rect.top) / rect.height) * 100;
        editorWrapper.style.height = `${Math.max(20, Math.min(80, pct))}%`;
      } else {
        const pct = ((e.clientX - rect.left) / rect.width) * 100;
        editorWrapper.style.flex = `0 0 ${Math.max(20, Math.min(80, pct))}%`;
      }
    });

    document.addEventListener("mouseup", () => {
      if (!dragging) return;
      dragging = false;
      divider.classList.remove("dragging");
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    });
  },

  initCodeEditor(el: HTMLElement) {
    editor = createEditor(el, this.resolvedTheme, this.editorMode === "vim", onEditorChange);
    loadSelectedExample(this.selectedExampleIndex);
  },

  onEditorModeChange() {
    localStorage.setItem("mpl-editorMode", this.editorMode);
    editor.setVimMode(this.editorMode === "vim");
  },

  cycleTheme() {
    const order: Theme[] = ["light", "system", "dark"];
    this.theme = order[(order.indexOf(this.theme) + 1) % order.length];
    this.onThemeChange();
  },

  onThemeChange() {
    localStorage.setItem("mpl-theme", this.theme);
    this.resolvedTheme = resolveTheme(this.theme);

    if (this.resolvedTheme === "dark") {
      document.documentElement.classList.add("dark-theme");
    } else {
      document.documentElement.classList.remove("dark-theme");
    }

    editor.setTheme(this.resolvedTheme);
    onEditorChange();
  },

  onExampleChange() {
    loadSelectedExample(this.selectedExampleIndex);
  },
};

Alpine.store("playground", playgroundStore);
Alpine.start();
