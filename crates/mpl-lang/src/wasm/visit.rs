//! Shared visitor/walker for pest parse trees.
//!
//! Follows the same visitor/walker separation as
//! `service::query::request::{visitor, walker}` but adapted for pest's
//! untyped `Pair<Rule>` tree rather than a typed AST.

use crate::parser::Rule;

use super::Span;

/// Controls whether the walker descends into children of the current pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VisitAction {
    /// Recurse into children of this pair.
    Walk,
    /// Skip children of this pair, continue to next sibling.
    Skip,
}

/// Snapshot of a pest pair's identity, captured before `into_inner()` consumes
/// it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Node {
    pub(super) rule: Rule,
    pub(super) span: Span,
}

/// Visitor hooks for walking a pest parse tree.
pub(super) trait PairVisitor {
    /// Called when entering a pair. Return [`VisitAction::Walk`] to recurse
    /// into children, or [`VisitAction::Skip`] to move to the next sibling.
    fn enter(&mut self, node: Node) -> VisitAction;

    /// Called after all children of a pair have been visited.
    /// Only called when [`Self::enter`] returned [`VisitAction::Walk`].
    fn leave(&mut self, _node: Node) {}

    /// Recursively walks a pest parse tree, calling [`Self::enter`] and
    /// [`Self::leave`] for each pair.
    fn walk_pairs(&mut self, pairs: pest::iterators::Pairs<'_, Rule>) {
        for pair in pairs {
            let node = Node {
                rule: pair.as_rule(),
                span: Span::new(pair.as_span().start(), pair.as_span().end()),
            };
            match self.enter(node) {
                VisitAction::Walk => {
                    self.walk_pairs(pair.into_inner());
                    self.leave(node);
                }
                VisitAction::Skip => {}
            }
        }
    }
}
