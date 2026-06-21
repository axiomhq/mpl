// Stub for @axiomhq/mpl used during unit tests.
// The real package is a WASM build artifact not committed to the repo.
// Tests that exercise pure-JS logic (needsEscape, escapeIdent, …) don't
// invoke any WASM functions, so returning undefined is sufficient.
export function completions(_query: string, _cursorPos: number, _systemParams?: unknown): unknown { return undefined; }
export function diagnostics(_query: string, _systemParams?: unknown): unknown { return undefined; }
export function tokenize(_query: string): unknown { return undefined; }
export function function_info(_label: string): unknown { return undefined; }
export function param_declarations(_query: string): unknown { return undefined; }
// Mirrors the Rust `IDENT` grammar (`[A-Za-z_][A-Za-z0-9_]*`). TEST DOUBLE
// only — production resolves `needsEscape` through the real wasm export, which
// is the single source of truth. Kept in sync with `mpl_lang::is_plain_ident`.
export function is_plain_ident(name: string): boolean { return /^[A-Za-z_][A-Za-z0-9_]*$/.test(name); }
export function extract_dataset(_query: string, _systemParams?: unknown): string | undefined { return undefined; }
export function parse_json(_query: string, _systemParams?: unknown): string { return ""; }
export function parse_ron(_query: string, _systemParams?: unknown): string { return ""; }
export function parse_wasm(_query: string, _systemParams?: unknown): unknown { return undefined; }
export function print_json(_query: string): string { return ""; }
export function print_ron(_query: string): string { return ""; }
export function initSync(_module: unknown): unknown { return {}; }
export default function init(_input?: unknown): Promise<unknown> { return Promise.resolve({}); }
