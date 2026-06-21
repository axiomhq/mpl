//! Lint rules for successfully-parsed MPL queries.
//!
//! Driven by the `winnow` lexer (`mpl_lang::wparser::highlight`) instead of a
//! `pest` tree-walk: a successful parse gates the lints, then the flat,
//! lossless token stream is scanned for the two lintable constructs
//! (`filter` keyword usage and unnecessary backtick escaping). This removes
//! the `PairVisitor`/`Rule` machinery the pest tree required.

use mpl_lang::wparser::{HlKind, highlight, parse_file};

use crate::Span;
use crate::diagnostics::{DiagnosticAction, DiagnosticItem, Severity};

fn filter_keyword_hint(span: Span) -> DiagnosticItem {
    DiagnosticItem {
        span,
        severity: Severity::Hint,
        message: "Consider using `where` instead of `filter`".to_string(),
        help: Some("`filter` is deprecated; `where` is preferred".to_string()),
        actions: vec![DiagnosticAction {
            name: "Replace with `where`".to_string(),
            span,
            insert: "where".to_string(),
        }],
    }
}

/// Returns `true` when `s` is a valid unescaped identifier per the
/// `plain_ident` grammar rule: starts with ASCII alpha, then any mix of
/// ASCII alphanumeric or `_`.
fn is_plain_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Builds the "unnecessary backtick escaping" hint for a `` `ident` `` token
/// whose inner text is a valid plain identifier; returns `None` otherwise.
fn unnecessary_escape_hint(span: Span, text: &str) -> Option<DiagnosticItem> {
    let inner = text.strip_prefix('`')?.strip_suffix('`')?;
    if inner.is_empty() || !is_plain_ident(inner) {
        return None;
    }
    Some(DiagnosticItem {
        span,
        severity: Severity::Hint,
        message: "Unnecessary backtick escaping".to_string(),
        help: Some(format!("`{inner}` is a valid unescaped identifier")),
        actions: vec![DiagnosticAction {
            name: "Remove backticks".to_string(),
            span,
            insert: inner.to_string(),
        }],
    })
}

/// Runs lint rules against a query and returns hint diagnostics. Lints only
/// fire when the query parses cleanly (no syntax/semantic errors), matching
/// the old "pest grammar succeeded" gate.
pub(crate) fn detect_hints(query: &str) -> Vec<DiagnosticItem> {
    let parsed = parse_file(query, Vec::new());
    if parsed.query.is_none() || !parsed.errors.is_empty() {
        return vec![];
    }

    let mut items = Vec::new();
    for token in highlight(query) {
        let text = &query[token.start..token.end];
        let span = Span::new(token.start, token.end);
        match token.kind {
            HlKind::Keyword if text == "filter" => items.push(filter_keyword_hint(span)),
            HlKind::Variable if text.starts_with('`') => {
                if let Some(item) = unnecessary_escape_hint(span, text) {
                    items.push(item);
                }
            }
            _ => {}
        }
    }
    items
}

#[cfg(test)]
mod tests;
