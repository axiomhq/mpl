//! Lint rules for successfully-parsed MPL queries.
//!
//! Driven by walking the lossless `rowan` CST. Hints are only emitted when the
//! query parses without recovery errors, matching the old pest behaviour
//! (`MPLParser::parse(...).ok()`).

use mpl_lang::cst::{self, SyntaxKind};
use rowan::NodeOrToken;

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

fn unnecessary_escape_hint(text: &str, span: Span) -> Option<DiagnosticItem> {
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

/// Runs lint rules against a query and returns any hint diagnostics.
///
/// Returns nothing when the query has parse-recovery errors, so a half-typed
/// query never produces spurious hints.
pub(crate) fn detect_hints(query: &str) -> Vec<DiagnosticItem> {
    let parse = cst::parse(query);
    if !parse.errors().is_empty() {
        return vec![];
    }

    let mut items = Vec::new();
    for token in parse
        .syntax()
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
    {
        let span = Span::new(
            token.text_range().start().into(),
            token.text_range().end().into(),
        );
        match token.kind() {
            SyntaxKind::KEYWORD if token.text() == "filter" => {
                items.push(filter_keyword_hint(span));
            }
            SyntaxKind::ESCAPED_IDENT => {
                if let Some(item) = unnecessary_escape_hint(token.text(), span) {
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
