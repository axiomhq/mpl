// Alpine.js store — glues editor, pipeline, and UI together.

import init, { parse_steps } from "@axiomhq/mpl-lang";
import Alpine from "alpinejs";
import { renderCharts, type ChartEntry } from "./charts";
import { initDatasets } from "./datasets";
import { createEditor, type EditorInstance } from "./editor";
import { interpret } from "./interpreter";

type Theme = "light" | "dark" | "system";

function resolveTheme(theme: Theme): "dark" | "light" {
  if (theme === "system") {
    return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
  }
  return theme;
}

function updateCharts(panel: HTMLElement, doc: string) {
  let result;
  try {
    result = parse_steps(doc);
  } catch (e) {
    renderCharts(panel, [{ label: doc.trim(), series: [], error: String(e) }]);
    return;
  }

  const { steps: seriesSteps, errors } = interpret(result.steps);

  const entries: ChartEntry[] = result.steps.map((step, i) => ({
    label: step.canonical,
    series: seriesSteps[i] ?? [],
    error: errors[i],
  }));

  renderCharts(panel, entries);
}

async function loadExamples(): Promise<string[]> {
  try {
    const resp = await fetch("/examples/index.json");
    if (!resp.ok) return [];
    return await resp.json();
  } catch {
    return [];
  }
}

const playgroundStore = {
  editorMode: (localStorage.getItem("mpl-editorMode") as "vim" | "normie") || "vim",
  theme: (localStorage.getItem("mpl-theme") as Theme) || "system",
  resolvedTheme: "light" as "dark" | "light",
  editor: null as EditorInstance | null,
  chartsPanel: null as HTMLElement | null,
  isInitialized: false,
  selectedExampleIndex: 0,
  examples: [] as string[],

  async init() {
    try {
      this.resolvedTheme = resolveTheme(this.theme);
      if (this.resolvedTheme === "dark") {
        document.documentElement.classList.add("dark-theme");
      }

      window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
        if (this.theme === "system") this.onThemeChange();
      });

      this.setupDivider();
      this.examples = await loadExamples();
      await Promise.all([init(), initDatasets()]);
      this.isInitialized = true;

      await this.loadSelectedExample();

      if (this.editor) {
        this.onEditorChange();
        this.editor.view.focus();
      }
    } catch (error) {
      console.error("Failed to initialize:", error);
    }
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
    this.editor = createEditor(el, this.resolvedTheme, this.editorMode === "vim", () =>
      this.onEditorChange(),
    );

    if (this.isInitialized) {
      this.loadSelectedExample();
    }
  },

  onEditorChange() {
    if (!this.isInitialized || !this.editor || !this.chartsPanel) return;
    updateCharts(this.chartsPanel, this.editor.view.state.doc.toString());
  },

  onEditorModeChange() {
    localStorage.setItem("mpl-editorMode", this.editorMode);
    this.editor?.setVimMode(this.editorMode === "vim");
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

    this.editor?.setTheme(this.resolvedTheme);
    this.onEditorChange();
  },

  async loadSelectedExample() {
    const exampleFile = this.examples[this.selectedExampleIndex];
    if (!exampleFile) return;

    try {
      const response = await fetch(`/examples/${exampleFile}`);
      if (!response.ok) throw new Error(`Failed to load example: ${response.statusText}`);
      const content = await response.text();

      if (this.editor) {
        this.editor.view.dispatch({
          changes: { from: 0, to: this.editor.view.state.doc.length, insert: content },
        });
        this.editor.view.focus();
      }
    } catch (error) {
      console.error("Error loading example:", error);
    }
  },

  async onExampleChange() {
    await this.loadSelectedExample();
  },
};

Alpine.store("playground", playgroundStore);
Alpine.start();
