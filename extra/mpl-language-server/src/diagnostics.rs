//! Diagnostics and code actions for `MPL` queries.
use std::collections::HashMap;

use miette::Diagnostic as _;
use serde::Serialize;
use strsim::jaro;

use mpl_lang::errors::Suggestion;
use mpl_lang::query::{ParamType, Warning, WarningReason};
use mpl_lang::{CompileError, GroupError, IfdefError, ParseError, TypeError, compile};

use crate::Span;
use crate::completions::{
    ALIGN_FN_NAMES, BUCKET_FN_NAMES, COMPUTE_FN_NAMES, GROUP_FN_NAMES, MAP_FN_NAMES,
};

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

#[derive(Clone, Serialize)]
pub struct DiagnosticAction {
    /// notification
    pub name: String,
    /// location to replace/insert
    #[serde(flatten)]
    pub span: Span,
    /// the string to insert/replace the span with
    pub insert: String,
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
pub struct DiagnosticItem {
    #[serde(flatten)]
    pub span: Span,
    pub severity: Severity,
    pub message: String,
    pub help: Option<String>,
    pub actions: Vec<DiagnosticAction>,
}

/// Returns diagnostics (errors, warnings, hints) for `query`.
///
/// `system_params` is the `HashMap` produced by
/// [`crate::to_compile_params`] from a host-supplied registration list
/// (`{ name, type, optional? }` entries). Pass an empty map to disable
/// system params — references then trip the regular `UndefinedParam` /
/// `ParamNotDeclared` warnings.
#[must_use]
pub fn compute_diagnostics(
    query: &str,
    system_params: &HashMap<String, ParamType>,
) -> Vec<DiagnosticItem> {
    match compile(query, system_params.clone()) {
        Ok((_, warnings)) => {
            let mut items: Vec<DiagnosticItem> = warnings
                .as_slice()
                .iter()
                .map(warning_to_diagnostic_item)
                .collect();
            items.extend(crate::lints::detect_hints(query));
            items
        }
        Err(CompileError::Parse(error)) => {
            let items = parse_error_diagnostic_items(&error);
            maybe_rewrite_escaped_dataset_error(query, items)
        }
        Err(CompileError::Type(error)) => type_error_diagnostic_items(&error),
        Err(CompileError::Group(error)) => group_error_diagnostic_items(&error),
        Err(CompileError::Ifdef(error)) => ifdef_error_diagnostic_items(&error),
    }
}

/// Variant of [`compute_diagnostics`] that takes ownership of the param
/// map; convenient for callers that build the map per request.
#[must_use]
pub fn compute_diagnostics_raw(
    query: &str,
    system_params: HashMap<String, ParamType>,
) -> Vec<DiagnosticItem> {
    compute_diagnostics(query, &system_params)
}

/// Convert a parser-emitted warning to a `DiagnosticItem`.
///
/// Each `WarningReason` variant is responsible for crafting its own
/// user-facing message, help text, and (where applicable) quick-fix action
/// — the parser's `Display` impl is intentionally not reused, so
/// editor-surfaced copy can be tuned without touching the core types.
pub fn warning_to_diagnostic_item(w: &Warning) -> DiagnosticItem {
    let span = w.source().map_or_else(
        || Span::new(0, 0),
        |s| Span::new(s.offset(), s.offset() + s.len()),
    );
    {
        match w.warning() {
            WarningReason::OldDuration => DiagnosticItem {
                span,
                severity: Severity::Warning,
                message: "`duration` is deprecated; use `Duration`".to_string(),
                help: Some(
                    "Param types use PascalCase: `Duration`, `Dataset`, `Regex`".to_string(),
                ),
                actions: vec![DiagnosticAction {
                    name: "Replace with `Duration`".to_string(),
                    span,
                    insert: "Duration".to_string(),
                }],
            },
            WarningReason::ParamNotDeclared(_) | WarningReason::ParamUsingSystemPrefix { .. } => {
                DiagnosticItem {
                    span,
                    severity: Severity::Warning,
                    message: w.warning().to_string(),
                    help: None,
                    actions: vec![],
                }
            }
        }
    }
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

pub fn type_error_diagnostic_items(e: &TypeError) -> Vec<DiagnosticItem> {
    let message = e.to_string();
    let help = e.help().map(|h| h.to_string());
    {
        if let Some(labels) = e.labels() {
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

pub fn ifdef_error_diagnostic_items(e: &IfdefError) -> Vec<DiagnosticItem> {
    let message = e.to_string();
    let help = e.help().map(|h| h.to_string());
    {
        let span = match e {
            IfdefError::OptionalOutsideOfIfdef { span, .. }
            | IfdefError::OptionalNotUsed { span, .. } => {
                Span::new(span.offset(), span.offset() + span.len())
            }
        };
        vec![DiagnosticItem {
            span,
            severity: Severity::Error,
            message,
            help,
            actions: vec![],
        }]
    }
}

pub fn group_error_diagnostic_items(e: &GroupError) -> Vec<DiagnosticItem> {
    let message = e.to_string();
    let help = e.help().map(|h| h.to_string());
    {
        let (prev_span, next_span) = match e {
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

pub fn parse_error_diagnostic_items(e: &ParseError) -> Vec<DiagnosticItem> {
    let message = e.to_string();
    let help = e.help().map(|h| h.to_string());
    let actions = parse_error_diagnostic_actions(e);
    {
        if let Some(labels) = e.labels() {
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
}

/// Extracts quick-fix actions by matching on the error variant and
/// fuzzy-matching against known function names or keywords.
fn parse_error_diagnostic_actions(e: &ParseError) -> Vec<DiagnosticAction> {
    {
        match e {
            ParseError::SyntaxError {
                span,
                suggestion: Some(suggestion),
                ..
            } => {
                vec![suggestion_to_diagnostic(
                    suggestion,
                    Span::new(span.offset(), span.offset() + span.len()),
                )]
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

/// Convert a parser-side suggestion into a code action over `span`.
fn suggestion_to_diagnostic(s: &Suggestion, span: Span) -> DiagnosticAction {
    DiagnosticAction::replace_with(span, s.suggestion())
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
