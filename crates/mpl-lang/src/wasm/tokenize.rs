//! Syntax highlighting tokenization for `MPL` queries.
use pest::Parser as _;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::parser::{MPLParser, Rule};

use super::Span;
use super::visit::{Node, PairVisitor, VisitAction};

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum TokenType {
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
pub(super) struct Token {
    #[serde(flatten)]
    pub(super) span: Span,
    #[serde(rename = "type")]
    pub(super) kind: TokenType,
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
        | Rule::kw_is
        | Rule::bucket_conversion
        | Rule::bucket_by_fn
        | Rule::bucket_by_with_conversion_fn => Some(TokenType::Keyword),
        Rule::param_type | Rule::param_native_type | Rule::tag_type => Some(TokenType::Type),
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

/// Collects tokens from a query string for testing.
#[cfg(test)]
pub(super) fn collect_tokens(query: &str) -> Option<Vec<Token>> {
    let pairs = MPLParser::parse(Rule::file, query).ok()?;
    let mut collector = TokenCollector {
        tokens: Vec::new(),
        source: query,
    };
    collector.walk_pairs(pairs);
    Some(collector.tokens)
}

/// Tokenizes a query string and returns an array of tokens for syntax highlighting.
#[must_use]
#[wasm_bindgen]
pub fn tokenize(query: &str) -> JsValue {
    let Ok(pairs) = MPLParser::parse(Rule::file, query) else {
        return JsValue::NULL;
    };
    let mut collector = TokenCollector {
        tokens: Vec::new(),
        source: query,
    };
    collector.walk_pairs(pairs);
    super::to_js_value(&collector.tokens)
}

#[cfg(test)]
#[allow(clippy::expect_fun_call)]
mod tests;
