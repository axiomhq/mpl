//! WebAssembly bindings for `mpl-language-server`.
//!
//! Thin shim layer: every `#[wasm_bindgen]` entry point here decodes its
//! `JsValue` arguments into pure Rust types, calls the corresponding
//! `mpl_language_server::*` function, and re-encodes the result.

use mpl_lang::{Query, compile, query::Source};
use mpl_language_server::SystemParamSpec;
use serde::Serialize;
use wasm_bindgen::prelude::*;

mod system_params;

/// Parse `query` (ignoring warnings) into a `Query` AST. Used by the
/// `parse_*` / `extract_dataset` shims below. Errors are stringified via
/// `Debug` to avoid pulling miette's fancy formatter into the wasm bundle.
fn parse_with_system_param_specs(
    query: &str,
    system_params: &[SystemParamSpec],
) -> Result<Query, String> {
    let params = mpl_language_server::to_compile_params(system_params);
    compile(query, params)
        .map(|(q, _warnings)| q)
        .map_err(|e| format!("{e:?}"))
}

/// Pure rust JSON parse helper.
fn parse_json_from_query(query: &str, system_params: &[SystemParamSpec]) -> Result<String, String> {
    let parsed = parse_with_system_param_specs(query, system_params)?;
    serde_json::to_string_pretty(&parsed).map_err(|e| format!("Failed to serialize to JSON: {e}"))
}

/// Pure rust RON parse helper.
fn parse_ron_from_query(query: &str, system_params: &[SystemParamSpec]) -> Result<String, String> {
    let parsed = parse_with_system_param_specs(query, system_params)?;
    ron::ser::to_string_pretty(&parsed, ron::ser::PrettyConfig::default())
        .map_err(|e| format!("Failed to serialize to RON: {e}"))
}

/// Pure rust parse helper.
fn extract_dataset_from_query(query: &str, system_params: &[SystemParamSpec]) -> Option<String> {
    fn get_dataset(q: &Query) -> String {
        match q {
            Query::Simple {
                source: Source { metric_id, .. },
                ..
            } => metric_id.dataset.to_string(),
            Query::Compute { left, .. } => get_dataset(left),
        }
    }

    let parsed = parse_with_system_param_specs(query, system_params).ok()?;
    Some(get_dataset(&parsed))
}

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

/// Extracts the dataset name from an `MPL` query string.
///
/// For `Simple` queries, returns the dataset from the source.
/// For `Compute` queries, recurses into the left-hand side.
/// Returns `None` if the query fails to parse.
#[must_use]
#[wasm_bindgen]
pub fn extract_dataset(query: &str, system_params: JsValue) -> Option<String> {
    let specs = system_params::decode(system_params);
    extract_dataset_from_query(query, &specs)
}

/// Returns the parameters a query declares inline (`param $x: int;`) as an
/// array of `{ name, type, optional }` objects.
///
/// Scans the query preamble, tolerating an incomplete query body, so params
/// are reported even while the query is still being written. Unlike the
/// `parse_*` shims this takes no `system_params`: host-injected params are not
/// *declared* by the query.
#[must_use]
#[wasm_bindgen]
pub fn declared_params(query: &str) -> JsValue {
    to_js_value(&mpl_language_server::declared_params(query))
}

/// Parses a query string into a `Query` AST encoded as a JS object.
#[wasm_bindgen]
pub fn parse_wasm(query: &str, system_params: JsValue) -> Result<JsValue, String> {
    let specs = system_params::decode(system_params);
    parse_with_system_param_specs(query, &specs).map(|q| to_js_value(&q))
}

/// Parses a query string into a JSON representation of the `Query` AST.
#[wasm_bindgen]
pub fn parse_json(query: &str, system_params: JsValue) -> Result<String, String> {
    let specs = system_params::decode(system_params);
    parse_json_from_query(query, &specs)
}

/// Parses a query string into a RON representation of the `Query` AST.
#[wasm_bindgen]
pub fn parse_ron(query: &str, system_params: JsValue) -> Result<String, String> {
    let specs = system_params::decode(system_params);
    parse_ron_from_query(query, &specs)
}

/// Converts a JSON representation of a `Query` back to an `MPL` query string.
#[wasm_bindgen]
pub fn print_json(query: &str) -> Result<String, String> {
    let query: Query =
        serde_json::from_str(query).map_err(|e| format!("Failed to deserialize from JSON: {e}"))?;
    Ok(query.to_string())
}

/// Converts a RON representation of a `Query` back to an `MPL` query string.
#[wasm_bindgen]
pub fn print_ron(query: &str) -> Result<String, String> {
    let query: Query =
        ron::de::from_str(query).map_err(|e| format!("Failed to deserialize from RON: {e}"))?;
    Ok(query.to_string())
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

#[cfg(test)]
mod tests {
    //! Native smoke tests for the string-in/string-out shims. The
    //! `JsValue`-returning shims (`parse_wasm`, etc.) require
    //! `wasm-bindgen-test` and are exercised by `tests/wasm/test-wasm.mjs`
    //! against the built wasm artifact.

    use super::{
        extract_dataset_from_query, parse_json_from_query, parse_ron_from_query, print_json,
        print_ron,
    };

    const QUERY: &str = "my_dataset:my_metric";

    #[test]
    fn extract_dataset_returns_dataset_for_simple_query() {
        assert_eq!(
            extract_dataset_from_query(QUERY, &[]),
            Some("my_dataset".to_string())
        );
    }

    #[test]
    fn extract_dataset_returns_none_for_invalid_query() {
        assert_eq!(extract_dataset_from_query("@@@ not a query @@@", &[]), None);
    }

    #[test]
    fn parse_print_json_roundtrips() {
        let json = parse_json_from_query(QUERY, &[]).expect("parse_json");
        let back = print_json(&json).expect("print_json");
        // The printed query should still reference the same dataset / metric.
        assert!(back.contains("my_dataset"));
        assert!(back.contains("my_metric"));
    }

    #[test]
    fn parse_print_ron_roundtrips() {
        let ron = parse_ron_from_query(QUERY, &[]).expect("parse_ron");
        let back = print_ron(&ron).expect("print_ron");
        assert!(back.contains("my_dataset"));
        assert!(back.contains("my_metric"));
    }

    #[test]
    fn parse_json_reports_error_for_invalid_query() {
        assert!(parse_json_from_query("@@@ not a query @@@", &[]).is_err());
    }
}
