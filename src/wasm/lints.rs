//! Lint rules for successfully-parsed MPL queries.

use pest::Parser as _;

use crate::parser::{MPLParser, Rule};

use super::diagnostics::{DiagnosticAction, DiagnosticItem, Severity};
use super::visit::{Node, PairVisitor, VisitAction};

/// A lint rule: when the walker encounters `rule`, `check` is called with
/// the node and source text. Return `Some` to emit a diagnostic.
struct LintRule {
    rule: Rule,
    check: fn(Node, &str) -> Option<DiagnosticItem>,
}

const LINT_RULES: &[LintRule] = &[
    LintRule {
        rule: Rule::kw_filter,
        check: lint_filter_keyword,
    },
    LintRule {
        rule: Rule::escaped_ident,
        check: lint_unnecessary_escape,
    },
    LintRule {
        rule: Rule::param_native_type,
        check: lint_lowercase_duration,
    },
];

#[allow(clippy::unnecessary_wraps)] // signature dictated by LintRule::check
fn lint_filter_keyword(node: Node, _source: &str) -> Option<DiagnosticItem> {
    Some(DiagnosticItem {
        span: node.span,
        severity: Severity::Hint,
        message: "Consider using `where` instead of `filter`".to_string(),
        help: Some("`filter` is deprecated; `where` is preferred".to_string()),
        actions: vec![DiagnosticAction {
            name: "Replace with `where`".to_string(),
            span: node.span,
            insert: "where".to_string(),
        }],
    })
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

/// Warns when the lowercase `duration` keyword is used as a param type.
/// `Duration` (capitalised) is the canonical form; `duration` is a legacy alias
/// kept in the grammar for backwards compatibility.
fn lint_lowercase_duration(node: Node, source: &str) -> Option<DiagnosticItem> {
    let text = &source[node.span.from..node.span.to];
    if text != "duration" {
        return None;
    }
    Some(DiagnosticItem {
        span: node.span,
        severity: Severity::Warning,
        message: "`duration` is deprecated; use `Duration`".to_string(),
        help: Some("Param types use PascalCase: `Duration`, `Dataset`, `Regex`".to_string()),
        actions: vec![DiagnosticAction {
            name: "Replace with `Duration`".to_string(),
            span: node.span,
            insert: "Duration".to_string(),
        }],
    })
}

fn lint_unnecessary_escape(node: Node, source: &str) -> Option<DiagnosticItem> {
    let text = &source[node.span.from..node.span.to];
    let inner = text.strip_prefix('`')?.strip_suffix('`')?;
    if inner.is_empty() || !is_plain_ident(inner) {
        return None;
    }
    Some(DiagnosticItem {
        span: node.span,
        severity: Severity::Hint,
        message: "Unnecessary backtick escaping".to_string(),
        help: Some(format!("`{inner}` is a valid unescaped identifier")),
        actions: vec![DiagnosticAction {
            name: "Remove backticks".to_string(),
            span: node.span,
            insert: inner.to_string(),
        }],
    })
}

struct LintVisitor<'a> {
    lints: &'a [LintRule],
    source: &'a str,
    items: Vec<DiagnosticItem>,
}

impl PairVisitor for LintVisitor<'_> {
    fn enter(&mut self, node: Node) -> VisitAction {
        for lint in self.lints {
            if node.rule == lint.rule {
                if let Some(item) = (lint.check)(node, self.source) {
                    self.items.push(item);
                }
                return VisitAction::Skip;
            }
        }
        VisitAction::Walk
    }
}

/// Runs lint rules against a successfully-parsed query and returns
/// any hint diagnostics.
pub(super) fn detect_hints(query: &str) -> Vec<DiagnosticItem> {
    let Ok(pairs) = MPLParser::parse(Rule::file, query) else {
        return vec![];
    };
    let mut visitor = LintVisitor {
        lints: LINT_RULES,
        source: query,
        items: Vec::new(),
    };
    visitor.walk_pairs(pairs);
    visitor.items
}

#[cfg(test)]
mod tests;
