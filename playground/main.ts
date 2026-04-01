import { EditorView, basicSetup } from "codemirror";
import { EditorState, Compartment } from "@codemirror/state";
import { oneDark } from "@codemirror/theme-one-dark";
import { vim } from "@replit/codemirror-vim";
import { json } from "@codemirror/lang-json";
import { rust } from "@codemirror/lang-rust";
import { mplHighlighter, createMplCompletion, mplLinter, mplSignatureHelp, mplHover } from "@axiomhq/mpl-codemirror";
import type { MplCompletionConfig } from "@axiomhq/mpl-codemirror";
import init, * as mpl from "@axiomhq/mpl-lang";
import Alpine from 'alpinejs';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';

type OutputFormat = 'json' | 'ron';
type EditorMode = 'vim' | 'normie';
type Theme = 'light' | 'dark';
type SyntaxMode = 'color' | 'plain';

// Example files served from public/examples/ (fetched at runtime)
const examples = [
  'align-rate.mpl',
  'as.mpl',
  'enrich.mpl',
  'filtered-histogram.mpl',
  'histogram.mpl',
  'histogram_rate.mpl',
  'map-gt.mpl',
  'map-mul.mpl',
  'nested-enrich.mpl',
  'params.mpl',
  'parser-error.mpl',
  'rate.mpl',
  'replace_labels.mpl',
  'set.mpl',
  'slo-histogram.mpl',
  'slo-ingest-rate.mpl',
  'slo.mpl',
  'sum_rate.mpl',
  'type-error.mpl',
];

// ── Dummy autocomplete data for the playground ─────────────────
//
// Each dataset has a set of metrics; each (dataset, metric) pair has a set of
// tags.  Some metrics and tags intentionally overlap across datasets so that
// completions feel realistic.

const DUMMY_DATASETS: Record<string, Record<string, string[]>> = {
  // Used by: align-rate, slo-ingest-rate, rate examples
  "dev.metrics": {
    "http_requests_total":                        ["method", "path", "code", "container", "status"],
    "transport_request_duration_seconds_bucket": ["status", "handler", "le"],
    "alertmanager_alerts":                              ["service", "namespace"],
  },
  // Used by: as, slo-ingest-rate examples
  "test-with-minus.com": {
    "ingest_pressure": ["time_window", "service"],
  },
  // Used by: most examples (slo, histogram, enrich, map-*, sum_rate, etc.)
  "test": {
    "http_requests_total":                        ["method", "path", "code", "container"],
    "http_request_duration_seconds_bucket":       ["method", "path", "code", "le", "handler"],
    "http_request_duration_seconds_count":        ["method", "path", "code", "container"],
    "http_server_request_duration_seconds_bucket":      ["method", "path", "le", "status"],
    "container_cpu_usage_seconds_total":                ["namespace", "pod", "container"],
    "kube_pod_container_resource_requests":             ["namespace", "pod", "resource"],
    "kube_pod_status_ready":                            ["pod"],
    "kube_pod_info":                                    ["pod", "node", "created_by_kind"],
    "kube_node_info":                                   ["node", "internal_ip"],
  },
  // Used by: params, set examples (generic placeholder)
  "dataset": {
    "metric": ["tag"],
  },
  // Extra datasets with values that need backtick-escaping
  "my app": {
    "request.latency_ms": ["endpoint", "region", "status_code", "service.name"],
    "queue.depth":        ["queue_name", "region"],
    "cache.hit_ratio":    ["cache_name", "region", "deployment.environment"],
    "error.rate":         ["endpoint", "error_type", "region"],
  },
  "otel": {
    "http_requests_total": ["method", "status", "service.name", "deployment.environment"],
    "rpc_duration":        ["method", "service.name", "rpc.system", "net.peer.name"],
    "db_query_duration":   ["db.system", "db.statement", "service.name"],
    "runtime.gc_pause":    ["service.name", "deployment.environment"],
  },
};

const dummyCompletionConfig: MplCompletionConfig = {
  datasets: () => Promise.resolve(Object.keys(DUMMY_DATASETS)),
  metrics: (dataset) =>
    Promise.resolve(Object.keys(DUMMY_DATASETS[dataset] ?? {})),
  tags: (dataset, metric) =>
    Promise.resolve(DUMMY_DATASETS[dataset]?.[metric] ?? []),
};

const mplCompletion = createMplCompletion(dummyCompletionConfig);

// Create the playground store object
const playgroundStore = {
  // State - load from localStorage or use defaults
  format: (localStorage.getItem('mpl-format') as OutputFormat) || 'ron',
  editorMode: (localStorage.getItem('mpl-editorMode') as EditorMode) || 'vim',
  theme: (localStorage.getItem('mpl-theme') as Theme) || 'dark',
  syntaxMode: (localStorage.getItem('mpl-syntaxMode') as SyntaxMode) || 'color',
  codeEditor: null as EditorView | null,
  astEditor: null as EditorView | null,
  terminal: null as Terminal | null,
  terminalFitAddon: null as FitAddon | null,
  codeVimCompartment: new Compartment(),
  astVimCompartment: new Compartment(),
  codeThemeCompartment: new Compartment(),
  astThemeCompartment: new Compartment(),
  astLanguageCompartment: new Compartment(),
  codeSyntaxCompartment: new Compartment(),
  astSyntaxCompartment: new Compartment(),
  codeCompletionCompartment: new Compartment(),
  codeDiagnosticsCompartment: new Compartment(),
  codeSignatureHelpCompartment: new Compartment(),
  codeHoverCompartment: new Compartment(),
  isUpdatingFromAst: false,
  isUpdatingFromCode: false,
  formatDisabled: false,
  isInitialized: false,
  selectedExampleIndex: 0,
  examples: examples,

  // Initialize the store (called automatically by Alpine)
  async init() {
    try {
      // Apply initial theme to document
      if (this.theme === 'dark') {
        document.documentElement.classList.add('dark-theme');
      }

      // Set up keyboard shortcuts
      this.setupKeyboardShortcuts();

      await init();
      this.isInitialized = true;

      // Load the selected example
      await this.loadSelectedExample();

      // Trigger initial parse once WASM is loaded and editor is ready
      if (this.codeEditor) {
        this.parseEditorContent(this.codeEditor.state.doc.toString());
        this.codeEditor.focus();
      }
    } catch (error) {
      this.writeToTerminal(`\x1b[1;31m✗ Failed to initialize:\x1b[0m\n${error}\n`);
    }
  },

  // Setup keyboard shortcuts for editor navigation
  setupKeyboardShortcuts() {
    document.addEventListener('keydown', (e: KeyboardEvent) => {
      // Ctrl+H: focus left editor (code)
      if (e.ctrlKey && e.key === 'h') {
        e.preventDefault();
        if (this.codeEditor) {
          this.codeEditor.focus();
        }
      }

      // Ctrl+L: focus right editor (ast)
      if (e.ctrlKey && e.key === 'l') {
        e.preventDefault();
        if (this.astEditor) {
          this.astEditor.focus();
        }
      }
    });
  },

  // Get vim extension based on current mode
  getVimExtension() {
    return this.editorMode === 'vim' ? vim() : [];
  },

  // Get theme extension based on current theme
  getThemeExtension() {
    return this.theme === 'dark' ? oneDark : [];
  },

  // Get language extension for AST editor based on format
  getAstLanguageExtension() {
    if (this.syntaxMode === 'plain') {
      return [];
    }
    if (this.format === 'json') {
      return json();
    } else if (this.format === 'ron') {
      // Use Rust syntax highlighting for RON (Rusty Object Notation)
      return rust();
    }
    return [];
  },

  // Get language extension for code editor (MPL)
  getCodeLanguageExtension() {
    if (this.syntaxMode === 'plain') {
      return [];
    }
    return mplHighlighter;
  },

  // Get extensions for editors
  getExtensions(isCodeEditor: boolean = true) {
    const vimCompartment = isCodeEditor ? this.codeVimCompartment : this.astVimCompartment;
    const themeCompartment = isCodeEditor ? this.codeThemeCompartment : this.astThemeCompartment;
    const syntaxCompartment = isCodeEditor ? this.codeSyntaxCompartment : this.astSyntaxCompartment;
    const extensions = [
      basicSetup,
      vimCompartment.of(this.getVimExtension()),
      themeCompartment.of(this.getThemeExtension()),
      EditorView.lineWrapping
    ];

    // Add language support based on editor type
    if (isCodeEditor) {
      extensions.push(syntaxCompartment.of(this.getCodeLanguageExtension()));
      extensions.push(this.codeCompletionCompartment.of(mplCompletion));
      extensions.push(this.codeDiagnosticsCompartment.of(mplLinter));
      extensions.push(this.codeSignatureHelpCompartment.of(mplSignatureHelp));
      extensions.push(this.codeHoverCompartment.of(mplHover));
    } else {
      extensions.push(syntaxCompartment.of(this.getAstLanguageExtension()));
    }

    if (isCodeEditor) {
      extensions.push(EditorView.updateListener.of((update) => {
        if (update.docChanged && !this.isUpdatingFromAst) {
          this.parseEditorContent(update.state.doc.toString());
        }
      }));
    } else {
      extensions.push(EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          this.printAstContent(update.state.doc.toString());
        }
      }));
    }

    return extensions;
  },

  // Initialize the code editor
  initCodeEditor(el: HTMLElement) {
    this.codeEditor = new EditorView({
      doc: "",  // Start with empty doc, will be populated by loadSelectedExample
      extensions: this.getExtensions(true),
      parent: el
    });

    // If WASM is already initialized, load the example and trigger parse
    if (this.isInitialized) {
      this.loadSelectedExample();
    }
  },

  // Initialize the AST editor
  initAstEditor(el: HTMLElement) {
    this.astEditor = new EditorView({
      state: EditorState.create({
        doc: "",
        extensions: this.getExtensions(false)
      }),
      parent: el
    });
  },

  // Initialize the terminal
  initTerminal(el: HTMLElement) {
    this.terminal = new Terminal({
      convertEol: true,
      cursorBlink: false,
      disableStdin: true,
      theme: this.theme === 'dark' ? {
        background: '#1e1e1e',
        foreground: '#e0e0e0',
      } : {
        background: '#ffffff',
        foreground: '#333333',
      }
    });

    this.terminalFitAddon = new FitAddon();
    this.terminal.loadAddon(this.terminalFitAddon);
    this.terminal.open(el);
    this.terminalFitAddon.fit();

    // Fit terminal on window resize
    window.addEventListener('resize', () => {
      if (this.terminalFitAddon) {
        this.terminalFitAddon.fit();
      }
    });

    // Write welcome message
    this.writeToTerminal('\x1b[1;36mMPL Playground\x1b[0m\n');
    this.writeToTerminal('Type or load an MPL query to see the parsed output.\n\n');
  },

  // Write to terminal
  writeToTerminal(text: string) {
    if (this.terminal) {
      this.terminal.write(text);
    }
  },

  // Clear terminal
  clearTerminal() {
    if (this.terminal) {
      this.terminal.clear();
    }
  },

  // Parse editor content to output format
  parseEditorContent(content: string) {
    if (!this.isInitialized) return;

    // Clear terminal for fresh output
    this.clearTerminal();

    try {
      const result = this.format === 'json'
        ? mpl.parse_json(content)
        : mpl.parse_ron(content);

      // Update AST editor with parsed result
      this.updateAst(result, false);

      // Write success message to terminal
      this.writeToTerminal('\x1b[1;32m✓ Parse successful!\x1b[0m\n');
      this.writeToTerminal(`Parsed to ${this.format.toUpperCase()} format.\n`);
    } catch (error) {
      // Clear the AST editor on error
      this.updateAst('', false);

      // Write error to terminal with colors
      this.writeToTerminal(`${error}\n`);
    }
  },

  // Print AST content back to code editor (reverse direction)
  printAstContent(content: string) {
    if (this.isUpdatingFromCode || !this.isInitialized) return;

    try {
      this.isUpdatingFromAst = true;
      const result = this.format === 'json'
        ? mpl.print_json(content)
        : mpl.print_ron(content);

       // Remove all backticks from the result
       const resultWithoutBackticks = result.replace(/`/g, '');

      // Update the code editor
      if (this.codeEditor) {
        this.codeEditor.dispatch({
          changes: {
            from: 0,
            to: this.codeEditor.state.doc.length,
            insert: resultWithoutBackticks
          }
        });
      }
      this.isUpdatingFromAst = false;

      // Clear terminal and show success
      this.clearTerminal();
      this.writeToTerminal('\x1b[1;32m✓ Converted to MPL!\x1b[0m\n');
    } catch (error) {
      // Clear the code editor on error (keep isUpdatingFromAst true to prevent triggering parse)
      if (this.codeEditor) {
        this.codeEditor.dispatch({
          changes: {
            from: 0,
            to: this.codeEditor.state.doc.length,
            insert: ''
          }
        });
      }

      this.isUpdatingFromAst = false;

      // Show error only in terminal
      this.clearTerminal();
      this.writeToTerminal('\x1b[1;31m✗ Error converting to MPL:\x1b[0m\n');
      this.writeToTerminal(`${error}\n`);
    }
  },

  // Update the AST editor content
  updateAst(content: string, isError: boolean = false) {
    if (this.astEditor && !this.isUpdatingFromAst) {
      this.isUpdatingFromCode = true;
      this.astEditor.dispatch({
        changes: {
          from: 0,
          to: this.astEditor.state.doc.length,
          insert: content
        }
      });
      this.isUpdatingFromCode = false;
    }

    // Disable format selector when there's an error
    this.formatDisabled = isError;
  },

  // Handle format change
  onFormatChange() {
    // Persist to localStorage
    localStorage.setItem('mpl-format', this.format);

    // Reconfigure AST editor with the appropriate language
    if (this.astEditor) {
      this.astEditor.dispatch({
        effects: this.astLanguageCompartment.reconfigure(this.getAstLanguageExtension())
      });
    }

    if (this.codeEditor && this.isInitialized) {
      this.parseEditorContent(this.codeEditor.state.doc.toString());
    }
  },

  // Handle editor mode change (vim/normie)
  onEditorModeChange() {
    // Persist to localStorage
    localStorage.setItem('mpl-editorMode', this.editorMode);

    // Reconfigure both editors with new vim mode settings
    const vimExtension = this.getVimExtension();

    if (this.codeEditor) {
      this.codeEditor.dispatch({
        effects: this.codeVimCompartment.reconfigure(vimExtension)
      });
    }

    if (this.astEditor) {
      this.astEditor.dispatch({
        effects: this.astVimCompartment.reconfigure(vimExtension)
      });
    }
  },

  // Handle theme change (light/dark)
  onThemeChange() {
    // Persist to localStorage
    localStorage.setItem('mpl-theme', this.theme);

    // Update document class for CSS theme
    if (this.theme === 'dark') {
      document.documentElement.classList.add('dark-theme');
    } else {
      document.documentElement.classList.remove('dark-theme');
    }

    // Reconfigure both editors with new theme
    const themeExtension = this.getThemeExtension();

    if (this.codeEditor) {
      this.codeEditor.dispatch({
        effects: this.codeThemeCompartment.reconfigure(themeExtension)
      });
    }

    if (this.astEditor) {
      this.astEditor.dispatch({
        effects: this.astThemeCompartment.reconfigure(themeExtension)
      });
    }

    // Update terminal theme
    if (this.terminal) {
      this.terminal.options.theme = this.theme === 'dark' ? {
        background: '#1e1e1e',
        foreground: '#e0e0e0',
      } : {
        background: '#ffffff',
        foreground: '#333333',
      };
    }
  },

  // Handle syntax mode change (color/plain)
  onSyntaxModeChange() {
    // Persist to localStorage
    localStorage.setItem('mpl-syntaxMode', this.syntaxMode);

    // Reconfigure both editors with new syntax highlighting
    const codeExtension = this.getCodeLanguageExtension();
    const astExtension = this.getAstLanguageExtension();

    if (this.codeEditor) {
      this.codeEditor.dispatch({
        effects: [
          this.codeSyntaxCompartment.reconfigure(codeExtension),
          this.codeCompletionCompartment.reconfigure(
            this.syntaxMode === 'plain' ? [] : mplCompletion
          ),
          this.codeDiagnosticsCompartment.reconfigure(
            this.syntaxMode === 'plain' ? [] : mplLinter
          ),
          this.codeSignatureHelpCompartment.reconfigure(
            this.syntaxMode === 'plain' ? [] : mplSignatureHelp
          ),
          this.codeHoverCompartment.reconfigure(
            this.syntaxMode === 'plain' ? [] : mplHover
          ),
        ]
      });
    }

    if (this.astEditor) {
      this.astEditor.dispatch({
        effects: this.astSyntaxCompartment.reconfigure(astExtension)
      });
    }
  },

  // Load the currently selected example into the editor
  async loadSelectedExample() {
    const exampleFile = this.examples[this.selectedExampleIndex];

    if (!exampleFile) return;

    // Fetch the example file
    try {
      const response = await fetch(`/examples/${exampleFile}`);
      if (!response.ok) {
        throw new Error(`Failed to load example: ${response.statusText}`);
      }
      const content = await response.text();

      if (this.codeEditor) {
        this.codeEditor.dispatch({
          changes: {
            from: 0,
            to: this.codeEditor.state.doc.length,
            insert: content
          }
        });
        this.codeEditor.focus();
      }

      // Clear terminal when loading new example
      this.clearTerminal();
    } catch (error) {
      console.error('Error loading example:', error);
      this.clearTerminal();
      this.writeToTerminal(`\x1b[1;31m✗ Error loading example:\x1b[0m\n${error}\n`);
    }
  },

  // Handle example selection change
  async onExampleChange() {
    // Load the selected example
    await this.loadSelectedExample();
  }
};

Alpine.store('playground', playgroundStore);
Alpine.start();
