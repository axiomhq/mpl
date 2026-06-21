//! Lint rules for successfully-parsed MPL queries.
//!
//! Driven off the `chumsky` lexer ([`mpl_lang::slice::highlight`]) plus a
//! parse-success gate ([`mpl_lang::slice::parse`]): lints only fire when the
//! query parses, and operate on the (total) token stream rather than a parse
//! tree. This replaces the former pest `PairVisitor` walk.

use mpl_lang::slice::{self, HlKind};

use crate::Span;
use crate::diagnostics::{DiagnosticAction, DiagnosticItem, Severity};

/// `filter` is the deprecated spelling of `where`.
fn lint_filter_keyword(span: Span) -> DiagnosticItem {
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

fn lint_unnecessary_escape(span: Span, source: &str) -> Option<DiagnosticItem> {
    let text = source.get(span.from..span.to)?;
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

/// Runs lint rules against a successfully-parsed query and returns
/// any hint diagnostics.
///
/// The parse-success gate matters: a query that does not parse (e.g. a
/// dataset with no metric) produces no hints, matching the old behavior.
pub(crate) fn detect_hints(query: &str) -> Vec<DiagnosticItem> {
    let parsed = slice::parse(query);
    if parsed.query.is_none() || !parsed.errors.is_empty() {
        return vec![];
    }

    let mut items = Vec::new();
    for token in slice::highlight(query) {
        let span = Span::new(token.from, token.to);
        match token.kind {
            HlKind::Keyword if query.get(token.from..token.to) == Some("filter") => {
                items.push(lint_filter_keyword(span));
            }
            HlKind::Variable => {
                if let Some(item) = lint_unnecessary_escape(span, query) {
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
