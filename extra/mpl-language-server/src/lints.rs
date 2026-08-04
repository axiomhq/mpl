//! Lint rules for MPL queries.
//!
//! Run over the syntax tree, so a rule fires on whatever the parser did manage
//! to recognise. Each rule keys on a *token kind* and then asks the tree what
//! the token means — that is what separates the `filter` keyword from a tag
//! that happens to be named `filter`.

use mpl_lang::syntax_tree::{Parser, SyntaxKind};
use rowan::NodeOrToken;

use crate::diagnostics::{DiagnosticAction, DiagnosticItem, Severity};
use crate::{Span, SyntaxToken};

/// A lint rule: when the walker reaches a token of `kind`, `check` is called
/// with it. Return `Some` to emit a diagnostic.
struct LintRule {
    kind: SyntaxKind,
    check: fn(&SyntaxToken) -> Option<DiagnosticItem>,
}

const LINT_RULES: &[LintRule] = &[
    LintRule {
        kind: SyntaxKind::LX_IDENT,
        check: lint_filter_keyword,
    },
    LintRule {
        kind: SyntaxKind::LX_ESCAPED_IDENT,
        check: lint_unnecessary_escape,
    },
];
// Note: lowercase `duration` is now reported by the parser itself as a
// `WarningReason::OldDuration` and surfaced via `Warning::to_diagnostic_item`.
// See `diagnostics.rs`.

/// The parser wraps a rule's leading word in a `KEYWORD` node, so requiring
/// that parent is what keeps a tag named `filter` from being flagged.
fn lint_filter_keyword(token: &SyntaxToken) -> Option<DiagnosticItem> {
    if token.text() != "filter"
        || !token
            .parent()
            .is_some_and(|p| p.kind() == SyntaxKind::KEYWORD)
    {
        return None;
    }
    let span = Span::from_text_range(token.text_range());
    Some(DiagnosticItem {
        span,
        severity: Severity::Hint,
        message: "Consider using `where` instead of `filter`".to_string(),
        help: Some("`filter` is deprecated; `where` is preferred".to_string()),
        actions: vec![DiagnosticAction {
            name: "Replace with `where`".to_string(),
            span,
            insert: "where".to_string(),
        }],
    })
}

fn lint_unnecessary_escape(token: &SyntaxToken) -> Option<DiagnosticItem> {
    let inner = token.text().strip_prefix('`')?.strip_suffix('`')?;
    if crate::needs_escape(inner) {
        return None;
    }
    let span = Span::from_text_range(token.text_range());
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

/// Runs the lint rules against `query` and returns any hint diagnostics.
pub(crate) fn detect_hints(query: &str) -> Vec<DiagnosticItem> {
    let (tree, _errors) = Parser::new(query).parse();
    tree.descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter_map(|token| {
            LINT_RULES
                .iter()
                .find(|lint| lint.kind == token.kind())
                .and_then(|lint| (lint.check)(&token))
        })
        .collect()
}

#[cfg(test)]
mod tests;
