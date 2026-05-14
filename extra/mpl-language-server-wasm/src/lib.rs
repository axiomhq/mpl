//! WebAssembly bindings for `mpl-language-server`.
//!
//! Thin shim layer: every `#[wasm_bindgen]` entry point here decodes its
//! `JsValue` arguments into pure Rust types, calls the corresponding
//! `mpl_language_server::*` function, and re-encodes the result.

use serde::Serialize;
use wasm_bindgen::prelude::*;

mod system_params;

/// Returns completion suggestions for the given cursor position.
///
/// `system_params` (optional) is an array of `{ name, type, optional? }`
/// objects describing parameters the host injects at runtime
/// (e.g. `$__interval`).
#[must_use]
#[wasm_bindgen]
pub fn completions(query: &str, cursor_pos: usize, system_params: JsValue) -> JsValue {
    let specs = system_params::decode(system_params);
    let extra = mpl_language_server::system_params_to_completion_items(&specs);
    let result = mpl_language_server::compute_completions_with_params(query, cursor_pos, &extra);
    to_js_value(&result)
}

/// Looks up a stdlib function by its qualified label (e.g. `"avg"` or
/// `"prom::rate"`) and returns its argument signature and documentation.
#[must_use]
#[wasm_bindgen]
pub fn function_info(label: &str) -> JsValue {
    to_js_value(&mpl_language_server::function_info(label))
}

/// Returns diagnostics (errors / warnings / lints) for `query`.
#[must_use]
#[wasm_bindgen]
pub fn diagnostics(query: &str, system_params: JsValue) -> JsValue {
    let specs = system_params::decode(system_params);
    let params = mpl_language_server::to_compile_params(&specs);
    let items = mpl_language_server::compute_diagnostics(query, &params);
    to_js_value(&items)
}

/// Tokenises `query` for syntax highlighting.
#[must_use]
#[wasm_bindgen]
pub fn tokenize(query: &str) -> JsValue {
    let tokens = mpl_language_server::collect_tokens(query);
    to_js_value(&tokens)
}

/// Returns the MPL language specification for LLMs. Available only when
/// the `examples` feature is enabled at build time.
#[cfg(feature = "examples")]
#[must_use]
#[wasm_bindgen]
pub fn query_spec() -> String {
    mpl_language_server::query_spec()
}

/// Serializes a value to `JsValue` using a JSON-compatible serializer.
///
/// `serde_wasm_bindgen::to_value` produces JS `Map` objects for types that
/// use `#[serde(flatten)]`, because serde routes those through
/// `serialize_map`. The `json_compatible()` serializer forces plain JS
/// objects instead, matching what the TypeScript consumers expect.
fn to_js_value(value: &impl Serialize) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}
