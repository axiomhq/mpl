//! WASM bindings for `MPL` parsing and serialization.
use serde::Serialize;
use wasm_bindgen::prelude::*;

mod completions;
mod diagnostics;
mod lints;
#[cfg(feature = "playground")]
pub mod steps;
mod tokenize;
mod visit;

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
