//! Syntax highlighting tokenization for `MPL` queries.
//!
//! Highlighting is driven by walking the lossless `rowan` CST from
//! [`mpl_lang::cst`]. Because the recursive-descent parser never fully fails
//! (it inserts error nodes), this returns tokens even for incomplete / invalid
//! mid-edit input — the property the old `pest`-based tokenizer could not
//! offer. The `SyntaxKind -> TokenType` map below is the single highlight
//! source of truth that replaces the JS regex grammar in `language.ts`.

use mpl_lang::cst::{self, SyntaxKind, SyntaxNode};
use rowan::NodeOrToken;
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

/// Maps a `SyntaxKind` to its highlight token type. `None` means "not
/// highlighted" (structural punctuation, trivia other than comments, …),
/// matching the previous pest behaviour.
fn token_type(kind: SyntaxKind) -> Option<TokenType> {
    Some(match kind {
        SyntaxKind::IDENT | SyntaxKind::PARAM_IDENT | SyntaxKind::ESCAPED_IDENT => {
            TokenType::Variable
        }
        SyntaxKind::STRING_FRAGMENT => TokenType::String,
        SyntaxKind::INT | SyntaxKind::FLOAT | SyntaxKind::INF_LIT | SyntaxKind::TIME_UNIT => {
            TokenType::Number
        }
        SyntaxKind::BOOL_LIT => TokenType::Bool,
        SyntaxKind::REGEX | SyntaxKind::REGEX_REPLACE => TokenType::Regexp,
        SyntaxKind::CMP_OP
        | SyntaxKind::EQ_EQ
        | SyntaxKind::BANG_EQ
        | SyntaxKind::LT_EQ
        | SyntaxKind::GT_EQ
        | SyntaxKind::PLUS
        | SyntaxKind::MINUS
        | SyntaxKind::STAR
        | SyntaxKind::SLASH => TokenType::Operator,
        SyntaxKind::PIPE => TokenType::Punctuation,
        SyntaxKind::KEYWORD => TokenType::Keyword,
        SyntaxKind::TYPE_NAME => TokenType::Type,
        SyntaxKind::COMMENT => TokenType::Comment,
        _ => return None,
    })
}

/// Classify a leaf token. A plain `IDENT` inside an error subtree (an
/// out-of-slice construct the parser did not recognise) falls back to the
/// centralized keyword table so `map`, `group`, types, etc. still light up.
fn classify(kind: SyntaxKind, text: &str, in_error: bool) -> Option<TokenType> {
    if kind == SyntaxKind::IDENT
        && in_error
        && let Some(resolved) = cst::keyword_syntax_kind(text)
    {
        return token_type(resolved);
    }
    token_type(kind)
}

fn span_of(range: rowan::TextRange) -> Span {
    Span::new(range.start().into(), range.end().into())
}

fn walk(node: &SyntaxNode, in_error: bool, tokens: &mut Vec<Token>) {
    for element in node.children_with_tokens() {
        match element {
            NodeOrToken::Node(child) => {
                // A relative time (`5m`) lexes as two tokens; emit it as one
                // Number (or Variable for `$param`) so it reads as a unit.
                if child.kind() == SyntaxKind::REL_TIME {
                    let is_param = child
                        .children_with_tokens()
                        .filter_map(NodeOrToken::into_token)
                        .any(|t| t.kind() == SyntaxKind::PARAM_IDENT);
                    tokens.push(Token {
                        span: span_of(child.text_range()),
                        kind: if is_param {
                            TokenType::Variable
                        } else {
                            TokenType::Number
                        },
                    });
                    continue;
                }
                walk(
                    &child,
                    in_error || child.kind() == SyntaxKind::ERROR_NODE,
                    tokens,
                );
            }
            NodeOrToken::Token(token) => {
                if let Some(kind) = classify(token.kind(), token.text(), in_error) {
                    tokens.push(Token {
                        span: span_of(token.text_range()),
                        kind,
                    });
                }
            }
        }
    }
}

/// Tokenises `query` for syntax highlighting by walking the CST.
///
/// Always returns `Some`: thanks to error recovery there is no
/// "failed to parse, show nothing" case any more — incomplete input still
/// yields the tokens recognised so far.
#[must_use]
pub fn collect_tokens(query: &str) -> Option<Vec<Token>> {
    let parse = cst::parse(query);
    let mut tokens = Vec::new();
    walk(&parse.syntax(), false, &mut tokens);
    Some(tokens)
}

#[cfg(test)]
mod tests;
