//! Diagnostics and code actions for `MPL` queries.
use miette::Diagnostic as _;
use serde::Serialize;
use strsim::jaro;
use wasm_bindgen::prelude::*;

use crate::errors::Suggestion;
use crate::{CompileError, GroupError, ParseError, TypeError, compile};

use super::Span;
use super::completions::{
    ALIGN_FN_NAMES, BUCKET_FN_NAMES, COMPUTE_FN_NAMES, GROUP_FN_NAMES, MAP_FN_NAMES,
};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Clone, Serialize)]
pub(super) struct DiagnosticAction {
    /// notification
    pub(super) name: String,
    /// location to replace/insert
    #[serde(flatten)]
    pub(super) span: Span,
    /// the string to insert/replace the span with
    pub(super) insert: String,
}

impl DiagnosticAction {
    fn replace_with(span: Span, suggestion: &str) -> DiagnosticAction {
        DiagnosticAction {
            name: format!("Replace with `{suggestion}`"),
            span,
            insert: suggestion.to_string(),
        }
    }
}

#[derive(Serialize)]
pub(super) struct DiagnosticItem {
    #[serde(flatten)]
    pub(super) span: Span,
    pub(super) severity: Severity,
    pub(super) message: String,
    pub(super) help: Option<String>,
    pub(super) actions: Vec<DiagnosticAction>,
}

/// Returns diagnostics (errors/warnings) for the given query string.
#[must_use]
#[wasm_bindgen]
pub fn diagnostics(query: &str) -> JsValue {
    let items = match compile(query) {
        Ok(_) => super::lints::detect_hints(query),
        Err(CompileError::Parse(error)) => {
            let items = error.diagnostic_items();
            maybe_rewrite_escaped_dataset_error(query, items)
        }
        Err(CompileError::Type(error)) => error.diagnostic_items(),
        Err(CompileError::Group(error)) => error.diagnostic_items(),
    };
    super::to_js_value(&items)
}

/// When the query starts with a backtick-escaped identifier containing `.`
/// that is not followed by `:`, rewrite the generic parse error to point at
/// the end of the identifier with a message about the missing metric name.
pub(crate) fn maybe_rewrite_escaped_dataset_error(
    query: &str,
    items: Vec<DiagnosticItem>,
) -> Vec<DiagnosticItem> {
    if items.len() != 1 || !matches!(items[0].severity, Severity::Error) {
        return items;
    }

    let Some(ident_end) = find_escaped_ident_end(query, 0) else {
        return items;
    };

    let inner = &query[1..ident_end - 1];

    // Only fire when the backtick ident is NOT followed by `:`
    let rest = query[ident_end..].trim_start();
    if rest.starts_with(':') {
        return items;
    }

    // The inner text has a dot — suggest dataset:metric syntax
    let Some(dot_pos) = inner.find('.') else {
        return items;
    };
    let dataset_part = &inner[..dot_pos];
    let metric_part = &inner[dot_pos + 1..];

    vec![DiagnosticItem {
        span: Span::new(ident_end, ident_end),
        severity: Severity::Error,
        message: "expected ':' and a metric name after the dataset".to_string(),
        help: Some(format!(
            "MPL uses ':' to separate dataset and metric, e.g. `{dataset_part}`:`{metric_part}`"
        )),
        actions: vec![],
    }]
}

/// Finds the byte position just past the closing backtick of an escaped
/// identifier starting at `start`. Returns `None` if no closing backtick.
fn find_escaped_ident_end(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.get(start) != Some(&b'`') {
        return None;
    }
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
        } else if bytes[i] == b'`' {
            return Some(i + 1);
        } else {
            i += 1;
        }
    }
    None
}

impl TypeError {
    pub(super) fn diagnostic_items(&self) -> Vec<DiagnosticItem> {
        let message = self.to_string();
        let help = self.help().map(|h| h.to_string());

        if let Some(labels) = self.labels() {
            let items: Vec<_> = labels
                .map(|label| {
                    let src = label.inner();
                    let span = Span::new(src.offset(), src.offset() + src.len());
                    let is_declaration = label.label().is_some_and(|l| l.contains("declaration"));

                    if is_declaration {
                        DiagnosticItem {
                            span,
                            severity: Severity::Info,
                            message: label.label().unwrap_or("declared here").to_string(),
                            help: None,
                            actions: vec![],
                        }
                    } else {
                        DiagnosticItem {
                            span,
                            severity: Severity::Error,
                            message: message.clone(),
                            help: help.clone(),
                            actions: vec![],
                        }
                    }
                })
                .collect();

            if items.is_empty() {
                vec![DiagnosticItem {
                    span: Span::new(0, 0),
                    severity: Severity::Error,
                    message,
                    help,
                    actions: vec![],
                }]
            } else {
                items
            }
        } else {
            vec![DiagnosticItem {
                span: Span::new(0, 0),
                severity: Severity::Error,
                message,
                help,
                actions: vec![],
            }]
        }
    }
}

impl GroupError {
    pub(super) fn diagnostic_items(&self) -> Vec<DiagnosticItem> {
        let message = self.to_string();
        let help = self.help().map(|h| h.to_string());
        let (prev_span, next_span) = match self {
            GroupError::InvalidGroups {
                prev_span,
                next_span,
                ..
            } => (
                Span::new(prev_span.offset(), prev_span.offset() + prev_span.len()),
                Span::new(next_span.offset(), next_span.offset() + next_span.len()),
            ),
        };
        vec![
            DiagnosticItem {
                span: prev_span,
                severity: Severity::Info,
                message: "previous groups declared here".to_string(),
                help: None,
                actions: vec![],
            },
            DiagnosticItem {
                span: next_span,
                severity: Severity::Error,
                message,
                help,
                actions: vec![],
            },
        ]
    }
}

impl ParseError {
    pub(super) fn diagnostic_items(&self) -> Vec<DiagnosticItem> {
        let message = self.to_string();
        let help = self.help().map(|h| h.to_string());
        let actions = self.diagnostic_actions();

        if let Some(labels) = self.labels() {
            let items: Vec<_> = labels
                .map(|label| {
                    let src = label.inner();
                    DiagnosticItem {
                        span: Span::new(src.offset(), src.offset() + src.len()),
                        severity: Severity::Error,
                        message: message.clone(),
                        help: help.clone(),
                        actions: actions.clone(),
                    }
                })
                .collect();

            if items.is_empty() {
                vec![DiagnosticItem {
                    span: Span::new(0, 0),
                    severity: Severity::Error,
                    message,
                    help,
                    actions,
                }]
            } else {
                items
            }
        } else {
            vec![DiagnosticItem {
                span: Span::new(0, 0),
                severity: Severity::Error,
                message,
                help,
                actions,
            }]
        }
    }

    /// Extracts quick-fix actions by matching on the error variant and
    /// fuzzy-matching against known function names or keywords.
    fn diagnostic_actions(&self) -> Vec<DiagnosticAction> {
        match self {
            ParseError::SyntaxError {
                span,
                suggestion: Some(suggestion),
                ..
            } => {
                vec![suggestion.to_diagnostic(Span::new(span.offset(), span.offset() + span.len()))]
            }

            ParseError::UnsupportedMapFunction { span, name }
            | ParseError::UnsupportedMapEvaluation { span, name } => {
                suggest_function_replacements(name, span.offset(), &MAP_FN_NAMES)
            }

            ParseError::UnsupportedAlignFunction { span, name } => {
                suggest_function_replacements(name, span.offset(), &ALIGN_FN_NAMES)
            }

            ParseError::UnsupportedGroupFunction { span, name } => {
                suggest_function_replacements(name, span.offset(), &GROUP_FN_NAMES)
            }

            ParseError::UnsupportedComputeFunction { span, name } => {
                suggest_function_replacements(name, span.offset(), &COMPUTE_FN_NAMES)
            }

            ParseError::UnsupportedBucketFunction { span, name } => {
                suggest_function_replacements(name, span.offset(), &BUCKET_FN_NAMES)
            }

            _ => vec![],
        }
    }
}

impl Suggestion {
    /// The suggested text
    fn to_diagnostic(&self, span: Span) -> DiagnosticAction {
        DiagnosticAction::replace_with(span, self.suggestion())
    }
}

/// Fuzzy-matches `input` against `candidates` using Jaro similarity and returns
/// up to 3 replacement actions for the best matches.
fn suggest_function_replacements(
    input: &str,
    from: usize,
    candidates: &[String],
) -> Vec<DiagnosticAction> {
    let input_lc = input.to_lowercase();
    let span = Span::new(from, from + input.len());
    let threshold = 0.8;

    let mut scored: Vec<_> = candidates
        .iter()
        .filter_map(|c| {
            let score = jaro(&input_lc, &c.to_lowercase());
            (score >= threshold).then(|| (c.clone(), score))
        })
        .collect();

    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(3);

    scored
        .into_iter()
        .map(|(name, _)| DiagnosticAction::replace_with(span, &name))
        .collect()
}

#[cfg(test)]
mod tests;
