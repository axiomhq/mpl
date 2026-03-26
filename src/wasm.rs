//! WASM bindings for `MPL` parsing and serialization.
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::{compile, query::Query};

mod completions;
mod diagnostics;
mod lints;
mod tokenize;
mod visit;

#[cfg(feature = "playground")]
fn format_error(source: &str, error: impl miette::Diagnostic + Send + Sync + 'static) -> String {
    use miette::{GraphicalReportHandler, GraphicalTheme, NamedSource};

    let mut output = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());

    let named_source = NamedSource::new("query.mpl", source.to_string());
    let report = miette::Report::new(error).with_source_code(named_source);

    if handler.render_report(&mut output, report.as_ref()).is_ok() {
        output
    } else {
        format!("{report:?}")
    }
}

#[cfg(feature = "playground")]
/// Parses a query string into a Query object.
#[wasm_bindgen]
pub fn parse_wasm(query: &str) -> Result<Query, String> {
    compile(query).map_err(|e| format_error(query, e))
}

#[cfg(feature = "playground")]
/// Parses a query string into a JSON representation of the Query object.
#[wasm_bindgen]
pub fn parse_json(query: &str) -> Result<String, String> {
    let parsed_query = compile(query).map_err(|e| format_error(query, e))?;
    serde_json::to_string_pretty(&parsed_query)
        .map_err(|e| format!("Failed to serialize to JSON: {e}"))
}

#[cfg(feature = "playground")]
/// Parses a query string into a RON representation of the Query object.
#[wasm_bindgen]
pub fn parse_ron(query: &str) -> Result<String, String> {
    let parsed_query = compile(query).map_err(|e| format_error(query, e))?;
    ron::ser::to_string_pretty(&parsed_query, ron::ser::PrettyConfig::default())
        .map_err(|e| format!("Failed to serialize to RON: {e}"))
}

/// Extracts the dataset name from an `MPL` query string.
///
/// For `Simple` queries, returns the dataset from the source.
/// For `Compute` queries, recurses into the left-hand side.
/// Returns `None` if the query fails to parse.
#[must_use]
#[wasm_bindgen]
pub fn extract_dataset(query: &str) -> Option<String> {
    fn get_dataset(q: &Query) -> String {
        match q {
            Query::Simple { source, .. } => source.metric_id.dataset.to_string(),
            Query::Compute { left, .. } => get_dataset(left),
        }
    }
    let parsed = compile(query).ok()?;
    Some(get_dataset(&parsed))
}

/// Converts a JSON representation of a Query back to `MPL` query string
#[wasm_bindgen]
pub fn print_json(query: &str) -> Result<String, String> {
    let query: Query =
        serde_json::from_str(query).map_err(|e| format!("Failed to deserialize from JSON: {e}"))?;
    Ok(query.to_string())
}

#[cfg(feature = "playground")]
/// Converts a RON representation of a Query back to `MPL` query string
#[wasm_bindgen]
pub fn print_ron(query: &str) -> Result<String, String> {
    let query: Query =
        ron::de::from_str(query).map_err(|e| format!("Failed to deserialize from RON: {e}"))?;
    Ok(query.to_string())
}

/// A (start, end) byte range matching `CodeMirror`'s convention.
/// Distinct from `miette::SourceSpan` which uses (offset, len).
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
struct Span {
    from: usize,
    to: usize,
}
impl Span {
    fn new(from: usize, to: usize) -> Self {
        Self { from, to }
    }
}

/// Returns the MPL language specification for LLMs.
///
/// Includes the language spec, standard library documentation, and examples
/// as a single markdown string — the same content served by the query service's
/// OPTIONS endpoint.
#[cfg(feature = "examples")]
#[must_use]
#[wasm_bindgen]
pub fn query_spec() -> String {
    use std::fmt::Write;

    use crate::{examples, stdlib::STDLIB};

    let stdlib_docs = STDLIB
        .documentation(1)
        .unwrap_or_else(|e| format!("**COULD NOT RENDER STDLIB DOCS**: {e}"));

    let examples_section = examples::MPL
        .iter()
        .fold(String::new(), |mut s, (name, example)| {
            let _ = write!(&mut s, "## {name}\n```\n{example}\n```\n");
            s
        });

    format!(
        "# MPL Metrics Query Specification\n\n{}\n\n# Standard library\n{}\n\n# Examples\n{}",
        examples::SPEC,
        stdlib_docs,
        examples_section
    )
}

/// Serializes a value to `JsValue` using a JSON-compatible serializer.
///
/// `serde_wasm_bindgen::to_value` produces JS `Map` objects for types that use
/// `#[serde(flatten)]`, because serde routes those through `serialize_map`.
/// The `json_compatible()` serializer forces plain JS objects instead, matching
/// what the TypeScript consumers expect.
fn to_js_value(value: &impl Serialize) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}
