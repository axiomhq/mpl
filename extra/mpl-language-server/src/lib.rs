//! Language services for MPL queries — completions, diagnostics,
//! tokenization, lints, hover. Pure Rust, no JS / wasm coupling.
//!
//! Hosts that want to expose these services to a JS frontend should depend
//! on `mpl-language-server-wasm`, which wraps this crate with
//! `#[wasm_bindgen]` shims.

mod completions;
mod declared_params;
mod diagnostics;
mod lints;
mod parser;
mod system_params;
mod tokenize;
mod visit;

pub use completions::{
    ALIGN_FN_NAMES, BUCKET_FN_NAMES, COMPUTE_FN_NAMES, CompletionArg, CompletionResult,
    FunctionInfo, FunctionItem, GROUP_FN_NAMES, KeywordItem, MAP_FN_NAMES, ParamItem, ParamType,
    compute_completions_with_params, function_info,
};
pub use declared_params::{DeclaredParam, declared_params};
pub use diagnostics::{
    DiagnosticAction, DiagnosticItem, Severity, compute_diagnostics, compute_diagnostics_raw,
};
pub use system_params::{
    SystemParamSpec, to_compile_params, to_completion_items as system_params_to_completion_items,
};
pub use tokenize::{Token, TokenType, collect_tokens};

/// Inclusive-exclusive byte range matching CodeMirror's convention.
/// Distinct from `miette::SourceSpan` which uses `(offset, len)`.
#[derive(Debug, Clone, Copy, serde::Serialize, PartialEq, Eq)]
pub struct Span {
    pub from: usize,
    pub to: usize,
}

impl Span {
    pub fn new(from: usize, to: usize) -> Self {
        Self { from, to }
    }
}

/// Returns the MPL language specification for LLMs.
///
/// Includes the language spec, standard library documentation, and examples
/// as a single markdown string — the same content served by the query
/// service's OPTIONS endpoint. Available only when `mpl-lang` is built with
/// the `examples` feature.
#[cfg(feature = "examples")]
#[must_use]
pub fn query_spec() -> String {
    use std::fmt::Write;

    use mpl_lang::STDLIB;

    let stdlib_docs = STDLIB
        .documentation(1)
        .unwrap_or_else(|e| format!("**COULD NOT RENDER STDLIB DOCS**: {e}"));

    let examples_section =
        mpl_lang::examples::MPL
            .iter()
            .fold(String::new(), |mut s, (name, example)| {
                let _ = write!(&mut s, "## {name}\n```\n{example}\n```\n");
                s
            });

    format!(
        "# MPL Metrics Query Specification\n\n{}\n\n# Standard library\n{}\n\n# Examples\n{}",
        mpl_lang::examples::SPEC,
        stdlib_docs,
        examples_section
    )
}
