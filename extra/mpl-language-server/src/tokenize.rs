//! Syntax highlighting tokenization for `MPL` queries.
//!
//! Driven by the `winnow` lexer in [`mpl_lang::wparser`] — a single,
//! span-accurate source of truth that tolerates incomplete / mid-edit input.
//! This replaces the previous parse-tree walk (which returned `None`
//! whenever the query failed to parse) and lets the editor highlight while the
//! user is still typing.
use mpl_lang::wparser::{HlKind, highlight};
use serde::Serialize;

use crate::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
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
    /// `// …` line comment. Previously highlighted by a regex in `language.ts`
    /// because the old grammar's comment rule was silent; now produced from
    /// Rust.
    Comment,
}

#[derive(Debug, Serialize)]
pub struct Token {
    #[serde(flatten)]
    pub span: Span,
    #[serde(rename = "type")]
    pub kind: TokenType,
}

/// Map a lexer [`HlKind`] to a highlight [`TokenType`]. Trivia (whitespace) and
/// unrecognised bytes carry no decoration and are dropped.
fn token_type(kind: HlKind) -> Option<TokenType> {
    Some(match kind {
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
        HlKind::Whitespace | HlKind::Unknown => return None,
    })
}

/// Tokenises `query` for syntax highlighting. The lexer is total, so this
/// always returns a token list — even for incomplete or invalid input.
#[must_use]
pub fn collect_tokens(query: &str) -> Vec<Token> {
    highlight(query)
        .into_iter()
        .filter_map(|token| {
            token_type(token.kind).map(|kind| Token {
                span: Span::new(token.start, token.end),
                kind,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests;
