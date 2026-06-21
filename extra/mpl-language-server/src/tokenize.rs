//! Syntax-highlighting tokenization for `MPL` queries.
//!
//! This is now a thin adapter over [`mpl_lang::slice::highlight`], the
//! `chumsky` lexer from the slice port. That lexer is **total** — it never
//! fails — so `collect_tokens` always returns spans, including for incomplete
//! / mid-edit input. This is what lets the editor (`language.ts`) drop its JS
//! regex fallback: highlighting no longer disappears the moment a query stops
//! parsing.

use mpl_lang::slice::{self, HlKind};
use serde::Serialize;

use crate::Span;

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    Variable,
    String,
    Number,
    Bool,
    Regexp,
    Operator,
    Punctuation,
    Keyword,
    Type,
    Comment,
}

#[derive(Debug, Serialize)]
pub struct Token {
    #[serde(flatten)]
    pub span: Span,
    #[serde(rename = "type")]
    pub kind: TokenType,
}

fn map_kind(kind: HlKind) -> TokenType {
    match kind {
        HlKind::Variable => TokenType::Variable,
        HlKind::String => TokenType::String,
        HlKind::Number => TokenType::Number,
        HlKind::Bool => TokenType::Bool,
        HlKind::Regexp => TokenType::Regexp,
        HlKind::Operator => TokenType::Operator,
        HlKind::Punctuation => TokenType::Punctuation,
        HlKind::Keyword => TokenType::Keyword,
        HlKind::Type => TokenType::Type,
        HlKind::Comment => TokenType::Comment,
    }
}

/// Tokenises `query` for syntax highlighting.
///
/// Always returns a (possibly empty) vector — never `None` — because the
/// underlying `chumsky` lexer survives arbitrary input.
#[must_use]
pub fn collect_tokens(query: &str) -> Vec<Token> {
    slice::highlight(query)
        .into_iter()
        .map(|h| Token {
            span: Span::new(h.from, h.to),
            kind: map_kind(h.kind),
        })
        .collect()
}

#[cfg(test)]
mod tests;
