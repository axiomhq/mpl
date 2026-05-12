import { Facet } from "@codemirror/state";

/**
 * Source-level type names accepted by the MPL parser for `param` declarations.
 * The system-params API mirrors this set verbatim so a host registration
 * reads like the language it shadows — `{ type: "Duration" }` is the same
 * token a user would have written inline.
 *
 * Lowercase entries are tag-value types; capitalised ones are language
 * built-ins. `duration` is accepted as a legacy alias but discouraged; the
 * canonical spelling is `Duration`.
 */
export type MplParamType =
  | "Dataset"
  | "Duration"
  | "Regex"
  | "string"
  | "int"
  | "float"
  | "bool";

/**
 * A host-supplied parameter the language server should treat as already
 * declared. Used for query-service variables like `$__interval` that are
 * injected at execution time but never written by the user.
 *
 * The name may be supplied with or without the leading `$`; the wasm bridge
 * normalises it. System params must use the `__` prefix — the parser
 * surfaces a diagnostic for any registration that doesn't.
 */
export interface MplSystemParam {
  name: string;
  type: MplParamType;
  optional?: boolean;
}

/**
 * Facet for registering system parameters with the MPL extensions.
 *
 * Hosts compose `mplSystemParams.of([...])` into their editor configuration;
 * `mplLinter`, `mplCompletion`, and `mplHover` all read from this facet so
 * a single registration informs every language-aware surface at once.
 *
 * Multiple providers (e.g. one per UI surface) are concatenated via
 * `combine: values => values.flat()`, so an editor can layer registrations
 * (global query-service params + an embedded widget's extra params) without
 * either side having to know about the other.
 *
 * Backwards compatibility: the facet defaults to `[]` when no provider is
 * present, so existing consumers behave exactly as before this addition.
 */
export const mplSystemParams = Facet.define<MplSystemParam[], MplSystemParam[]>({
  combine: values => values.flat(),
});
