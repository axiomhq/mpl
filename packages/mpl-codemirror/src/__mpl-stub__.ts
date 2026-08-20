// Stub for @axiomhq/mpl used during unit tests.
// The real package is a WASM build artifact not committed to the repo.
// Tests that exercise pure-JS logic (needsEscape, escapeIdent, …) don't
// invoke any WASM functions, so returning undefined is sufficient. Logic that
// lives in Rust is tested there; see `extra/mpl-language-server`.
export function completions(_query: string, _cursorPos: number, _systemParams?: unknown): unknown { return undefined; }
export function diagnostics(_query: string, _systemParams?: unknown): unknown { return undefined; }
export function tokenize(_query: string): unknown { return undefined; }
export function token_at(_query: string, _offset: number): unknown { return undefined; }
export function declared_params(_query: string): unknown { return undefined; }
export function keyword_info(_label: string): unknown { return undefined; }
export function needs_escape(_name: string): boolean { return false; }
export function escape_ident(name: string): string { return name; }
export function apply_text_for_ident(name: string, _inBacktick: boolean): string { return name; }
export function function_info(_label: string): unknown { return undefined; }
export function extract_dataset(_query: string, _systemParams?: unknown): string | undefined { return undefined; }
export function parse_json(_query: string, _systemParams?: unknown): string { return ""; }
export function parse_ron(_query: string, _systemParams?: unknown): string { return ""; }
export function parse_wasm(_query: string, _systemParams?: unknown): unknown { return undefined; }
export function print_json(_query: string): string { return ""; }
export function initSync(_module: unknown): unknown { return {}; }
export default function init(_input?: unknown): Promise<unknown> { return Promise.resolve({}); }
