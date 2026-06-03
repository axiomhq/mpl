import { autocompletion, Completion, CompletionContext, CompletionResult, snippet } from "@codemirror/autocomplete";
import { EditorState } from "@codemirror/state";
import * as mpl from "@axiomhq/mpl";
import { type WasmArgType, formatArgs } from "./wasm-types";
import { CompletionCache } from "./completion-cache";
import { mplSystemParams } from "./system-params";

/**
 * Snippet template for the `ifdef` keyword. Inserts the full canonical
 * surface form — `ifdef($<name>) { where <cursor> }` — with tab stops at
 * the param name and the body. Teaches the canonical shape (always `where`,
 * never `filter`) in one stroke; users tab through the placeholders rather
 * than discovering each piece via three sequential completions.
 *
 * The leading `$` is a literal because the snippet parser only treats
 * `${...}` (or `#{...}`) as a placeholder; a bare `$` followed by another
 * `$` falls through as text.
 */
export const IFDEF_SNIPPET = snippet("ifdef($${name}) { where ${} }");

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
  optional: boolean;
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

/**
 * The engine returns an array of results. Most positions yield zero or one
 * result; an empty `expr` position (e.g. inside `${ }`, after `where tag ==`,
 * or `extend foo =`) yields both a `params` and a `tag` result so the editor
 * can offer params and tags at once and let its own prefix filter separate
 * `$param` references from bare tag names.
 */
type WasmCompletions = WasmCompletionResult[] | null;

function callCompletions(doc: string, pos: number, systemParams: unknown): WasmCompletions {
  try {
    const raw = mpl.completions(doc, pos, systemParams) as unknown;
    // Tolerate older single-object payloads defensively; normalise to an array.
    if (raw == null) {
      return null;
    }
    return (Array.isArray(raw) ? raw : [raw]) as WasmCompletionResult[];
  } catch {
    // WASM not ready or error
    return null;
  }
}

/**
 * Tag/dataset/metric options come from the host unfiltered, so CodeMirror
 * filters them; the engine already prefix-filters the inline kinds (params,
 * keywords, functions), so those are passed through verbatim.
 */
function filterForKind(kind: WasmCompletionResult["kind"]): boolean {
  return kind === "tag" || kind === "dataset" || kind === "metric";
}

/**
 * Builds options for the kinds the engine fully describes inline (params,
 * keywords, stdlib functions). Returns `[]` for host-resolved kinds (tag,
 * dataset, metric), which each source handles itself.
 */
function inlineOptions(result: WasmCompletionResult): Completion[] {
  if (result.kind === "params") {
    return result.options.map(item => ({
      label: item.label,
      type: "variable" as const,
      detail: item.optional ? `Option<${item.type}>` : item.type,
    }));
  }
  if (result.kind === "keywords") {
    return result.options.map(mapKeywordItem);
  }
  if (
    result.kind === "align_functions" ||
    result.kind === "map_functions" ||
    result.kind === "group_functions" ||
    result.kind === "bucket_functions" ||
    result.kind === "compute_functions"
  ) {
    return result.options.map(item => ({
      label: item.label,
      type: "function" as const,
      detail: formatArgs(item.args),
      info: item.info,
    }));
  }
  return [];
}

/**
 * Combines per-result option lists into a single CodeMirror result. A merged
 * (multi-result) completion always lets CodeMirror filter the union so the
 * typed prefix selects params or tags; a single result keeps that kind's
 * native filtering.
 */
function combine(results: WasmCompletionResult[], optionLists: Completion[][]): CompletionResult | null {
  const options = optionLists.flat();
  if (options.length === 0) {
    return null;
  }
  const filter = results.length > 1 ? true : filterForKind(results[0].kind);
  return { from: results[0].from, to: results[0].to, options, filter };
}

function mplCompletionSource(context: CompletionContext): CompletionResult | null {
  const doc = context.state.doc.toString();
  const systemParams = context.state.facet(mplSystemParams);
  const results = callCompletions(doc, context.pos, systemParams);
  if (!results || results.length === 0) {
    return null;
  }

  // Placeholder source: tag/dataset/metric are not connected to an API, so
  // emit a single descriptive placeholder for each.
  const optionLists = results.map((result): Completion[] => {
    if (result.kind === "tag") {
      return [{
        label: `<tag for ${result.dataset}:${result.metric}>`,
        type: "variable",
        info: "Tag completions not yet connected",
      }];
    }
    if (result.kind === "dataset") {
      return [{ label: "<dataset>", type: "variable", info: "Dataset completions not yet connected" }];
    }
    if (result.kind === "metric") {
      return [{ label: `<metric for ${result.dataset}>`, type: "variable", info: "Metric completions not yet connected" }];
    }
    return inlineOptions(result);
  });

  const merged = combine(results, optionLists);
  if (!merged) {
    return null;
  }
  // Placeholder labels are descriptive (`<tag for …>`) and would be filtered
  // out by prefix matching, so never let CodeMirror filter this source.
  return { ...merged, filter: false };
}

/**
 * Builds a CodeMirror `Completion` from a wasm `KeywordItem`.
 *
 * Special-cases `ifdef` to use a snippet that inserts the full canonical
 * surface form (param name + `where` body). Every other keyword gets the
 * literal `apply` text from the wasm payload, or no `apply` at all when the
 * wasm side did not supply one.
 */
function mapKeywordItem(item: WasmKeywordItem): Completion {
  if (item.label === "ifdef") {
    return {
      label: item.label,
      apply: IFDEF_SNIPPET,
      type: "keyword" as const,
      info: item.info,
    };
  }
  return {
    label: item.label,
    ...(item.apply ? { apply: item.apply } : {}),
    type: "keyword" as const,
    info: item.info,
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

export function createMplCompletionSource(config: MplCompletionConfig) {
  const datasetCache = new CompletionCache<string[]>(config.cacheTtlMs);
  const metricCache = new CompletionCache<string[]>(config.cacheTtlMs);
  const tagCache = new CompletionCache<string[]>(config.cacheTtlMs);

  /**
   * Resolves one engine result into CodeMirror options, fetching host data
   * for tag/dataset/metric kinds. A failed host fetch yields `[]` so a
   * sibling result (e.g. the params half of a merged completion) still shows.
   */
  async function optionsForResult(result: WasmCompletionResult, doc: string): Promise<Completion[]> {
    if (result.kind === "tag") {
      try {
        const cacheKey = `${result.dataset}\0${result.metric}`;
        let tags = tagCache.get(cacheKey);
        if (!tags) {
          tags = await config.tags(result.dataset, result.metric);
          tagCache.set(cacheKey, tags);
        }
        const inBacktick = result.from > 0 && doc.charAt(result.from - 1) === "`";
        return tags.map(t => {
          const apply = applyTextForIdent(t, inBacktick);
          return apply !== t
            ? { label: t, apply, type: "variable" as const }
            : { label: t, type: "variable" as const };
        });
      } catch {
        return [];
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
        return datasets.map(d => {
          const apply = applyTextForIdent(d, inBacktick);
          return apply !== d
            ? { label: d, apply, type: "variable" as const }
            : { label: d, type: "variable" as const };
        });
      } catch {
        return [];
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
        return metrics.map(m => {
          const apply = applyTextForIdent(m, inBacktick);
          return apply !== m
            ? { label: m, apply, type: "variable" as const }
            : { label: m, type: "variable" as const };
        });
      } catch {
        return [];
      }
    }

    return inlineOptions(result);
  }

  return async (context: CompletionContext): Promise<CompletionResult | null> => {
    const doc = context.state.doc.toString();
    const systemParams = context.state.facet(mplSystemParams);
    const results = callCompletions(doc, context.pos, systemParams);
    if (!results || results.length === 0) {
      return null;
    }

    const optionLists = await Promise.all(results.map(result => optionsForResult(result, doc)));
    return combine(results, optionLists);
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
