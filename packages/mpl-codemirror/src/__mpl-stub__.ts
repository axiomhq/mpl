// Stub for @axiomhq/mpl used during unit tests.
// The real package is a WASM build artifact not committed to the repo.
// Tests that exercise pure-JS logic (needsEscape, escapeIdent, …) don't
// invoke any WASM functions, so returning undefined is sufficient.
export function completions(_query: string, _cursorPos: number, _systemParams?: unknown): unknown { return undefined; }
export function diagnostics(_query: string, _systemParams?: unknown): unknown { return undefined; }
export function tokenize(_query: string): unknown { return undefined; }
export function function_info(_label: string): unknown { return undefined; }
export function initSync(_module: unknown): unknown { return {}; }
export default function init(_input?: unknown): Promise<unknown> { return Promise.resolve({}); }
