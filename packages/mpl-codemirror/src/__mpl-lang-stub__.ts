// Stub for @axiomhq/mpl used during unit tests.
// The real package is a WASM build artifact not committed to the repo.
// Tests that exercise pure-JS logic (needsEscape, escapeIdent, …) don't
// invoke any WASM functions, so returning undefined is sufficient.
export function completions(_query: string, _cursorPos: number): unknown { return undefined; }
export function diagnostics(_query: string): unknown { return undefined; }
export function tokenize(_query: string): unknown { return undefined; }
export function function_info(_label: string): unknown { return undefined; }
export function extract_dataset(_query: string): string | undefined { return undefined; }
export function parse_json(_query: string): string { return ""; }
export function parse_ron(_query: string): string { return ""; }
export function parse_wasm(_query: string): unknown { return undefined; }
export function print_json(_query: string): string { return ""; }
export function print_ron(_query: string): string { return ""; }
export function initSync(_module: unknown): unknown { return {}; }
export default function init(_input?: unknown): Promise<unknown> { return Promise.resolve({}); }
