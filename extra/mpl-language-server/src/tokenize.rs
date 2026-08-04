//! Syntax highlighting tokenization for `MPL` queries.
//!
//! Built on the error-tolerant syntax tree rather than on a full parse, so a
//! query that is half-typed still highlights. Anything the parser could not
//! make sense of lands in an error node, whose tokens are still coloured by
//! their lexical kind — the file never loses its colours mid-edit.

use mpl_lang::syntax_tree::{Parser, SyntaxKind, SyntaxNode};
use rowan::{NodeOrToken, TextRange, TextSize, TokenAtOffset, WalkEvent};
use serde::Serialize;

use crate::{Span, SyntaxToken};

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

/// The words the parser dispatches on. Consulted only inside error nodes, where
/// there is no structure left to ask.
const RULE_KEYWORDS: &[&str] = &[
    "align", "and", "as", "bucket", "by", "compute", "else", "extend", "filter", "ifdef", "in",
    "is", "map", "not", "or", "param", "sample", "set", "to", "using", "where",
];

/// What an identifier means is decided by the node that encloses it, not by the
/// identifier itself: `duration` is a type in a `param` declaration, a keyword
/// after `is`, and a tag name anywhere else. Walk out until a node decides,
/// passing through the wrappers that only restate "this is an identifier".
fn ident_token_type(token: &SyntaxToken) -> TokenType {
    for node in token.parent_ancestors() {
        match node.kind() {
            SyntaxKind::KEYWORD | SyntaxKind::BUCKET_ARG => return TokenType::Keyword,
            // The parser gave up here, so there is no enclosing construct to
            // consult — fall back to the word itself. Confined to error nodes
            // on purpose, so a tag legitimately named `where` in a query that
            // parses keeps its variable colour.
            SyntaxKind::GARBAGE | SyntaxKind::INVALID => {
                return if RULE_KEYWORDS.contains(&token.text()) {
                    TokenType::Keyword
                } else {
                    TokenType::Variable
                };
            }
            SyntaxKind::TYPE | SyntaxKind::OTEL_TYPE => return TokenType::Type,
            // The `m` of `1m` is an identifier to the lexer but part of the
            // number to a reader. Reached by point queries; `collect_tokens`
            // claims the whole duration before it gets here.
            SyntaxKind::TIME_UNIT => return TokenType::Number,
            // Bucket function names are a closed vocabulary in the grammar
            // (`histogram`, `interpolate_cumulative_histogram`), so they read as
            // keywords. Every other function path is a free name.
            SyntaxKind::FUNCTION_PATH => {
                return if node
                    .parent()
                    .is_some_and(|p| p.kind() == SyntaxKind::BUCKET)
                {
                    TokenType::Keyword
                } else {
                    TokenType::Variable
                };
            }
            SyntaxKind::IDENT
            | SyntaxKind::IDENT_OR_VARIABLE
            | SyntaxKind::VARIABLE
            | SyntaxKind::EXPR
            | SyntaxKind::CONST => {}
            _ => break,
        }
    }
    TokenType::Variable
}

/// The kind a single token is painted with.
fn token_token_type(token: &SyntaxToken) -> Option<TokenType> {
    use SyntaxKind::{
        LX_BOOL, LX_COMMENT, LX_DIV, LX_EQUAL_EQUAL, LX_ESCAPED_IDENT, LX_ESCAPED_VARIABLE,
        LX_FLOAT, LX_GREATER_THAN, LX_GREATER_THAN_EQUAL, LX_IDENT, LX_INF, LX_INTEGER,
        LX_LESS_THAN, LX_LESS_THAN_EQUAL, LX_MINUS, LX_MUL, LX_NOT_EQUAL, LX_PIPE, LX_PLUS,
        LX_REGEX, LX_STRING, LX_STRING_SEGMENT, LX_VARIABLE, TYPE,
    };

    Some(match token.kind() {
        LX_IDENT | LX_ESCAPED_IDENT | LX_VARIABLE | LX_ESCAPED_VARIABLE => ident_token_type(token),
        // An interpolated literal is several tokens: each run of literal text
        // carries its own `${` or `}` delimiter, and the expressions between
        // them are coloured on their own.
        LX_STRING | LX_STRING_SEGMENT => TokenType::String,
        LX_INTEGER | LX_FLOAT | LX_INF => TokenType::Number,
        LX_BOOL => TokenType::Bool,
        LX_REGEX => TokenType::Regexp,
        LX_COMMENT => TokenType::Comment,
        LX_PIPE => TokenType::Punctuation,
        LX_EQUAL_EQUAL | LX_NOT_EQUAL | LX_PLUS | LX_MINUS | LX_MUL | LX_DIV => TokenType::Operator,
        // In `Option<...>` the angle brackets delimit a type; everywhere else
        // they compare.
        LX_LESS_THAN | LX_LESS_THAN_EQUAL | LX_GREATER_THAN | LX_GREATER_THAN_EQUAL
            if !token.parent().is_some_and(|p| p.kind() == TYPE) =>
        {
            TokenType::Operator
        }
        _ => return None,
    })
}

/// A node's range covers the trivia the parser ate on the way in and out, so
/// using it verbatim would colour the whitespace around the construct. Narrow
/// to the span of the tokens that carry text.
fn content_range(node: &SyntaxNode) -> Option<TextRange> {
    let mut tokens = node
        .descendants_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(|t| !matches!(t.kind(), SyntaxKind::LX_WHITESPACE | SyntaxKind::LX_COMMENT))
        .map(|t| t.text_range());
    let first = tokens.next()?;
    Some(tokens.last().map_or(first, |last| first.cover(last)))
}

/// Tokenises `query` for syntax highlighting.
///
/// Always succeeds: an unparseable query yields the tokens it does have rather
/// than nothing at all. Tokens come out sorted by start offset and never
/// overlap, which is what CodeMirror's decoration builder requires.
#[must_use]
pub fn collect_tokens(query: &str) -> Vec<Token> {
    let (tree, _errors) = Parser::new(query).parse();
    let mut tokens = Vec::new();
    let mut walk = tree.preorder_with_tokens();
    while let Some(event) = walk.next() {
        let WalkEvent::Enter(element) = event else {
            continue;
        };
        match element {
            // A duration is a digit and a unit to the lexer but one number to a
            // reader, so it is the one construct claimed whole.
            NodeOrToken::Node(node) if node.kind() == SyntaxKind::DURATION => {
                if let Some(range) = content_range(&node) {
                    tokens.push(Token {
                        span: Span::from_text_range(range),
                        kind: TokenType::Number,
                    });
                }
                walk.skip_subtree();
            }
            NodeOrToken::Node(_) => {}
            NodeOrToken::Token(token) => {
                if let Some(kind) = token_token_type(&token) {
                    tokens.push(Token {
                        span: Span::from_text_range(token.text_range()),
                        kind,
                    });
                }
            }
        }
    }
    tokens
}

/// The token at byte `offset`, for point queries like hover.
///
/// A `::`-qualified function name is reported whole rather than as the segment
/// under the cursor, because that is the name the stdlib is keyed by — hovering
/// `rate` in `prom::rate` has to look up `prom::rate`.
///
/// Returns `None` where there is nothing to say: whitespace, punctuation, or an
/// offset past the end of the query. Callers read the name out of their own copy
/// of the text with the returned span.
#[must_use]
pub fn token_at(query: &str, offset: usize) -> Option<Token> {
    let offset = TextSize::new(u32::try_from(offset).ok()?);
    let (tree, _errors) = Parser::new(query).parse();
    if !tree.text_range().contains_inclusive(offset) {
        return None;
    }

    let token = match tree.token_at_offset(offset) {
        TokenAtOffset::None => return None,
        TokenAtOffset::Single(token) => token,
        // On a boundary, take the token the offset points *into*. Hover asks
        // about the character under the pointer, and that is the right one.
        TokenAtOffset::Between(_, right) => right,
    };

    let kind = token_token_type(&token)?;
    // Report the same spans `collect_tokens` paints, so a caller holding both
    // never sees one construct two ways: a qualified path is one name, and a
    // duration is one number.
    let range = token
        .parent_ancestors()
        .find(|n| matches!(n.kind(), SyntaxKind::FUNCTION_PATH | SyntaxKind::DURATION))
        .and_then(|node| content_range(&node))
        .unwrap_or_else(|| token.text_range());

    Some(Token {
        span: Span::from_text_range(range),
        kind,
    })
}

#[cfg(test)]
mod tests;
