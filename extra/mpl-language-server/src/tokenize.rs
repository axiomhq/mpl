//! Syntax highlighting tokenization for `MPL` queries.
use mpl_lang::{MPLParser, Rule};
use pest::Parser as _;
use serde::Serialize;

use crate::Span;
use crate::visit::{Node, PairVisitor, VisitAction};

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
}

#[derive(Debug, Serialize)]
pub struct Token {
    #[serde(flatten)]
    pub span: Span,
    #[serde(rename = "type")]
    pub kind: TokenType,
}

/// Returns `Option` rather than adding a `None` variant to `TokenType` because
/// the absence drives control flow in the visitor (recurse into children)
/// and `TokenType` is serialized directly to the JS consumer.
fn token_type(rule: Rule) -> Option<TokenType> {
    match rule {
        Rule::plain_ident | Rule::escaped_ident | Rule::param_ident => Some(TokenType::Variable),
        Rule::string => Some(TokenType::String),
        Rule::float
        | Rule::int
        | Rule::time_relative
        | Rule::time_rfc_3339
        | Rule::time_timestamp
        | Rule::time_modifier => Some(TokenType::Number),
        Rule::bool => Some(TokenType::Bool),
        Rule::regex | Rule::regex_replace => Some(TokenType::Regexp),
        Rule::cmp | Rule::cmp_re | Rule::map_calc_op | Rule::compute_op => {
            Some(TokenType::Operator)
        }
        Rule::pipe_keyword => Some(TokenType::Punctuation),
        Rule::kw_not
        | Rule::kw_filter
        | Rule::kw_where
        | Rule::kw_sample
        | Rule::kw_ifdef
        | Rule::kw_else
        | Rule::kw_is
        | Rule::bucket_conversion
        | Rule::bucket_by_fn
        | Rule::bucket_by_with_conversion_fn => Some(TokenType::Keyword),
        Rule::param_native_type | Rule::tag_type => Some(TokenType::Type),
        _ => None,
    }
}

struct TokenCollector<'a> {
    tokens: Vec<Token>,
    source: &'a str,
}

impl PairVisitor for TokenCollector<'_> {
    fn enter(&mut self, node: Node) -> VisitAction {
        // `time_relative_parameterized` can be either `1m` (Number) or `$dur`
        // (Variable). Inspect the source text to decide.
        if node.rule == Rule::time_relative_parameterized {
            let text = &self.source[node.span.from..node.span.to];
            let kind = if text.starts_with('$') {
                TokenType::Variable
            } else {
                TokenType::Number
            };
            self.tokens.push(Token {
                span: node.span,
                kind,
            });
            return VisitAction::Skip;
        }

        // `param` is a compound rule (`"param" ~ param_ident ~ ":" ~ param_type ~ ";"`).
        // The literal "param" keyword is not a named child, so emit a keyword
        // token for it and let the walker descend into the named children.
        if node.rule == Rule::param {
            let kw_start = node.span.from;
            self.tokens.push(Token {
                span: Span::new(kw_start, kw_start + "param".len()),
                kind: TokenType::Keyword,
            });
            return VisitAction::Walk;
        }

        // `optional_type` matches `Option<…>`; the literal "Option" is not a
        // named child, so emit a Type token for it and walk into the inner
        // type rule for its own token.
        if node.rule == Rule::optional_type {
            let start = node.span.from;
            self.tokens.push(Token {
                span: Span::new(start, start + "Option".len()),
                kind: TokenType::Type,
            });
            return VisitAction::Walk;
        }

        if let Some(kind) = token_type(node.rule) {
            self.tokens.push(Token {
                span: node.span,
                kind,
            });
            VisitAction::Skip
        } else {
            VisitAction::Walk
        }
    }
}

/// Tokenises `query` for syntax highlighting. Returns `None` when the query
/// fails to parse (the host should treat that as "no tokens to show").
#[must_use]
pub fn collect_tokens(query: &str) -> Option<Vec<Token>> {
    let pairs = MPLParser::parse(Rule::file, query).ok()?;
    let mut collector = TokenCollector {
        tokens: Vec::new(),
        source: query,
    };
    collector.walk_pairs(pairs);
    Some(collector.tokens)
}

#[cfg(test)]
mod tests;
