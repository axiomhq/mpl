import { autocompletion, CompletionContext, CompletionResult } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import * as mpl from "@axiomhq/mpl";
import { type WasmArgType, formatArgs } from "./wasm-types";
import { CompletionCache } from "./completion-cache";

/**
 * Registers `` ` `` and `$` as word characters so that CodeMirror's
 * `activateOnTyping` triggers the completion source when the user types
 * either character (backtick for escaped identifiers, `$` for params).
 */
export const mplWordChars = EditorState.languageData.of(() => [{ wordChars: "`$" }]);

interface WasmKeywordItem {
  label: string;
  apply?: string;
  info: string;
}

interface WasmFunctionItem {
  label: string;
  args: { name: string; type: WasmArgType }[];
  info: string;
}

interface WasmKeywordResult {
  kind: "keywords";
  from: number;
  to: number;
  options: WasmKeywordItem[];
}

interface WasmFunctionResult {
  kind: "align_functions" | "map_functions" | "group_functions" | "bucket_functions" | "compute_functions";
  from: number;
  to: number;
  options: WasmFunctionItem[];
}

interface WasmTagCompletion {
  kind: "tag";
  from: number;
  to: number;
  dataset: string;
  metric: string;
}

interface WasmDatasetCompletion {
  kind: "dataset";
  from: number;
  to: number;
}

interface WasmMetricCompletion {
  kind: "metric";
  from: number;
  to: number;
  dataset: string;
}

type WasmParamType = "dataset" | "metric" | "duration" | "string" | "int" | "float" | "bool" | "regex";

interface WasmParamItem {
  label: string;
  type: WasmParamType;
}

interface WasmParamResult {
  kind: "params";
  from: number;
  to: number;
  options: WasmParamItem[];
}

type WasmCompletionResult =
  | WasmKeywordResult
  | WasmFunctionResult
  | WasmTagCompletion
  | WasmDatasetCompletion
  | WasmMetricCompletion
  | WasmParamResult;

function mplCompletionSource(context: CompletionContext): CompletionResult | null {
  const doc = context.state.doc.toString();
  let result: WasmCompletionResult | null = null;
  try {
    result = mpl.completions(doc, context.pos) as WasmCompletionResult | null;
  } catch {
    // WASM not ready or error
    return null;
  }

  if (!result) {
    return null;
  }

  // Parameter completions — declared $variables
  if (result.kind === "params") {
    if (result.options.length === 0) {
      return null;
    }
    return {
      from: result.from,
      to: result.to,
      options: result.options.map(item => ({
        label: item.label,
        type: "variable" as const,
        detail: item.type,
      })),
      filter: false,
    };
  }

  // Tag completions — placeholder labels until the host environment supplies real tag names
  if (result.kind === "tag") {
    return {
      from: result.from,
      to: result.to,
      options: [
        { label: `<tag for ${result.dataset}:${result.metric}>`, type: "variable", info: "Tag completions not yet connected" },
      ],
      filter: false,
    };
  }

  // Dataset completions — placeholder until connected to an API
  if (result.kind === "dataset") {
    return {
      from: result.from,
      to: result.to,
      options: [
        { label: "<dataset>", type: "variable", info: "Dataset completions not yet connected" },
      ],
      filter: false,
    };
  }

  // Metric completions — placeholder until connected to an API
  if (result.kind === "metric") {
    return {
      from: result.from,
      to: result.to,
      options: [
        { label: `<metric for ${result.dataset}>`, type: "variable", info: "Metric completions not yet connected" },
      ],
      filter: false,
    };
  }

  if (result.options.length === 0) {
    return null;
  }

  // Rust already filters by partial word; let WASM be the source of truth.
  if (result.kind === "keywords") {
    return {
      from: result.from,
      to: result.to,
      options: result.options.map(item => ({
        label: item.label,
        ...(item.apply ? { apply: item.apply } : {}),
        type: "keyword" as const,
        info: item.info,
      })),
      filter: false,
    };
  }

  return {
    from: result.from,
    to: result.to,
    options: result.options.map(item => ({
      label: item.label,
      type: "function" as const,
      detail: formatArgs(item.args),
      info: item.info,
    })),
    filter: false,
  };
}

export const mplCompletion = [
  mplWordChars,
  autocompletion({
    override: [mplCompletionSource],
  }),
];

export interface MplCompletionConfig {
  datasets: () => Promise<string[]>;
  metrics: (dataset: string) => Promise<string[]>;
  tags: (dataset: string, metric: string) => Promise<string[]>;
  cacheTtlMs?: number;
}

function createMplCompletionSource(config: MplCompletionConfig) {
  const datasetCache = new CompletionCache<string[]>(config.cacheTtlMs);
  const metricCache = new CompletionCache<string[]>(config.cacheTtlMs);
  const tagCache = new CompletionCache<string[]>(config.cacheTtlMs);

  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const doc = context.state.doc.toString();
    let result: WasmCompletionResult | null = null;
    try {
      result = mpl.completions(doc, context.pos) as WasmCompletionResult | null;
    } catch {
      return null;
    }

    if (!result) {
      return null;
    }

    if (result.kind === "params") {
      if (result.options.length === 0) {
        return null;
      }
      return {
        from: result.from,
        to: result.to,
        options: result.options.map(item => ({
          label: item.label,
          type: "variable" as const,
          detail: item.type,
        })),
        filter: false,
      };
    }

    if (result.kind === "tag") {
      try {
        const cacheKey = `${result.dataset}\0${result.metric}`;
        let tags = tagCache.get(cacheKey);
        if (!tags) {
          tags = await config.tags(result.dataset, result.metric);
          tagCache.set(cacheKey, tags);
        }
        const inBacktick = result.from > 0 && doc.charAt(result.from - 1) === "`";
        return {
          from: result.from,
          to: result.to,
          options: tags.map(t => {
            const apply = applyTextForIdent(t, inBacktick);
            return apply !== t
              ? { label: t, apply, type: "variable" as const }
              : { label: t, type: "variable" as const };
          }),
          filter: true,
        };
      } catch {
        return null;
      }
    }

    if (result.kind === "dataset") {
      try {
        let datasets = datasetCache.get("");
        if (!datasets) {
          datasets = await config.datasets();
          datasetCache.set("", datasets);
        }
        const inBacktick = result.from > 0 && doc.charAt(result.from - 1) === "`";
        return {
          from: result.from,
          to: result.to,
          options: datasets.map(d => {
            const apply = applyTextForIdent(d, inBacktick);
            return apply !== d
              ? { label: d, apply, type: "variable" as const }
              : { label: d, type: "variable" as const };
          }),
          filter: true,
        };
      } catch {
        return null;
      }
    }

    if (result.kind === "metric") {
      try {
        let metrics = metricCache.get(result.dataset);
        if (!metrics) {
          metrics = await config.metrics(result.dataset);
          metricCache.set(result.dataset, metrics);
        }
        const inBacktick = result.from > 0 && doc.charAt(result.from - 1) === "`";
        return {
          from: result.from,
          to: result.to,
          options: metrics.map(m => {
            const apply = applyTextForIdent(m, inBacktick);
            return apply !== m
              ? { label: m, apply, type: "variable" as const }
              : { label: m, type: "variable" as const };
          }),
          filter: true,
        };
      } catch {
        return null;
      }
    }

    if (result.options.length === 0) {
      return null;
    }

    if (result.kind === "keywords") {
      return {
        from: result.from,
        to: result.to,
        options: result.options.map(item => ({
          label: item.label,
          ...(item.apply ? { apply: item.apply } : {}),
          type: "keyword" as const,
          info: item.info,
        })),
        filter: false,
      };
    }

    return {
      from: result.from,
      to: result.to,
      options: result.options.map(item => ({
        label: item.label,
        type: "function" as const,
        detail: formatArgs(item.args),
        info: item.info,
      })),
      filter: false,
    };
  };
}

export function createMplCompletion(config: MplCompletionConfig) {
  return [
    mplWordChars,
    autocompletion({
      override: [createMplCompletionSource(config)],
    }),
  ];
}

const PLAIN_IDENT_RE = /^[A-Za-z][A-Za-z0-9_]*$/;

export function needsEscape(name: string): boolean {
  return !PLAIN_IDENT_RE.test(name);
}

export function escapeIdent(name: string): string {
  if (!needsEscape(name)) {
    return name;
  }
  return "`" + name.replace(/\\/g, "\\\\").replace(/`/g, "\\`") + "`";
}

/**
 * Builds the `apply` text for a dataset/metric/tag completion.
 *
 * When the user has already typed an opening backtick (detected by checking
 * the character before `from` in the document), the apply text must NOT
 * include a second opening backtick — it only appends the name + closing
 * backtick. Otherwise a double-backtick is inserted.
 */
export function applyTextForIdent(name: string, inBacktick: boolean): string {
  const escaped = name.replace(/\\/g, "\\\\").replace(/`/g, "\\`");
  if (inBacktick) {
    return escaped + "`";
  }
  if (needsEscape(name)) {
    return "`" + escaped + "`";
  }
  return name;
}
