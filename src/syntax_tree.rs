use std::{fmt::Display, iter::Peekable};

use miette::{Diagnostic, SourceSpan};
use rowan::GreenNodeBuilder;

use crate::lexer::{Lexer, Token, TokenType};

/// syntax errors
#[derive(thiserror::Error, Debug, Diagnostic)]
pub enum SyntaxError {
    /// Unexpected end of file.
    #[error("unexpected end of file")]
    #[diagnostic(code(mpl_lang::unexpected_eof))]
    Eof {
        /// The range of the EOF token.
        #[label("unexpected end of file")]
        range: SourceSpan,
    },
    /// Token found after the end of query.
    #[error("unexpected {kind:?} after end of query")]
    #[diagnostic(code(mpl_lang::generic_syntax_error))]
    TokenAfterEoq {
        /// The kind of the token found after the end of query.
        kind: TokenType,
        /// The range of the token found after the end of query.
        #[label("unexpedted {kind:?} after end of query")]
        range: SourceSpan,
    },
    /// Generic syntax error.
    #[error("generic syntax error: {message}")]
    #[diagnostic(code(mpl_lang::generic_syntax_error))]
    Generic {
        /// The error message.
        message: String,
        /// The range of the error.
        #[label("{message}")]
        range: SourceSpan,
    },
}
/// The language definition for the MPL language syntax tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lang {}
impl rowan::Language for Lang {
    type Kind = SyntaxKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        *ALL_KINDS
            .get(raw.0 as usize)
            .unwrap_or(&SyntaxKind::THIS_SHOULD_NEVER_BE_EMITTED_GOD_DAMN_IT)
    }
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// A syntax node in the MPL language syntax tree.
pub type SyntaxNode = rowan::SyntaxNode<Lang>;

const ALL_KINDS: [SyntaxKind; ROOT as usize + 1] = [
    EOF,
    LX_INVALID,
    LX_COMMENT,
    LX_WHITESPACE,
    LX_IDENT,
    LX_ESCAPED_IDENT,
    LX_DIV,
    LX_MUL,
    LX_PLUS,
    LX_MINUS,
    LX_PIPE,
    LX_DOUBLE_COLON,
    LX_COLON,
    LX_INTEGER,
    LX_FLOAT,
    LX_EQUAL_EQUAL,
    LX_EQUAL,
    LX_VARIABLE,
    LX_ESCAPED_VARIABLE,
    LX_REGEX,
    LX_COMMA,
    LX_L_PAREN,
    LX_R_PAREN,
    LX_L_BRACKET,
    LX_R_BRACKET,
    LX_L_BRACE,
    LX_R_BRACE,
    LX_QUESTION_MARK,
    LX_BANG,
    LX_SEMI_COLON,
    LX_LESS_THAN_EQUAL,
    LX_GREATER_THAN_EQUAL,
    LX_LESS_THAN,
    LX_GREATER_THAN,
    LX_NOT_EQUAL,
    LX_DOT_DOT,
    LX_STRING,
    LX_STRING_SEGMENT,
    LX_BOOL,
    LX_INF,
    IDENT,
    IDENT_OR_VARIABLE,
    KEYWORD,
    DIRECTIVE,
    PARAM,
    VARIABLE,
    TYPE,
    QUERY,
    SIMPLE_QUERY,
    COMPUTE_QUERY,
    FILTER_OR,
    FILTER_AND,
    FILTER_NOT,
    FILTER_PAREN,
    FILTER_CMP,
    FUNCTION_PATH,
    DURATION,
    TIME_UNIT,
    OTEL_TYPE,
    EXPR,
    REGEX,
    CONST,
    INTEGER,
    FLOAT,
    BOOL,
    STRING,
    ARRAY,
    TAG_LIST,
    TIME_RANGE,
    TIME,
    RULE,
    EXTEND,
    EXTEND_PART,
    IFDEF,
    FILTER,
    SAMPLE,
    MAP,
    MAP_MATH,
    ALIGN,
    AS,
    GROUP,
    BUCKET,
    BUCKET_ARG,
    BUCKET_ARGS,
    INVALID,
    GARBAGE,
    THIS_SHOULD_NEVER_BE_EMITTED_GOD_DAMN_IT,
    ROOT,
];

/// The syntax kind of a node in the MPL language syntax tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms, missing_docs)]
#[repr(u16)]
pub enum SyntaxKind {
    EOF = 0,
    // Lexer tokens
    LX_INVALID,
    LX_COMMENT,
    LX_WHITESPACE,
    LX_IDENT,
    LX_ESCAPED_IDENT,
    LX_DIV,
    LX_MUL,
    LX_PLUS,
    LX_MINUS,
    LX_PIPE,
    LX_DOUBLE_COLON,
    LX_COLON,
    LX_INTEGER,
    LX_FLOAT,
    LX_EQUAL_EQUAL,
    LX_EQUAL,
    LX_VARIABLE,
    LX_ESCAPED_VARIABLE,
    LX_REGEX,
    LX_COMMA,
    LX_L_PAREN,
    LX_R_PAREN,
    LX_L_BRACKET,
    LX_R_BRACKET,
    LX_L_BRACE,
    LX_R_BRACE,
    LX_QUESTION_MARK,
    LX_BANG,
    LX_SEMI_COLON,
    LX_LESS_THAN_EQUAL,
    LX_GREATER_THAN_EQUAL,
    LX_LESS_THAN,
    LX_GREATER_THAN,
    LX_NOT_EQUAL,
    LX_DOT_DOT,
    LX_STRING,
    LX_STRING_SEGMENT,
    LX_BOOL,
    LX_INF,

    IDENT,
    IDENT_OR_VARIABLE,
    KEYWORD,
    DIRECTIVE,
    PARAM,
    VARIABLE,
    TYPE,
    QUERY,
    SIMPLE_QUERY,
    COMPUTE_QUERY,
    FILTER_OR,
    FILTER_AND,
    FILTER_NOT,
    FILTER_PAREN,
    FILTER_CMP,
    FUNCTION_PATH,
    DURATION,
    TIME_UNIT,
    OTEL_TYPE,

    EXPR,
    REGEX,
    CONST,
    INTEGER,
    FLOAT,
    BOOL,
    STRING,
    ARRAY,
    TAG_LIST,
    TIME_RANGE,
    TIME,

    RULE,
    EXTEND,
    EXTEND_PART,
    IFDEF,
    FILTER,
    SAMPLE,
    MAP,
    MAP_MATH,
    ALIGN,
    AS,
    GROUP,
    BUCKET,
    BUCKET_ARG,
    BUCKET_ARGS,

    /// invalid in the syntax tree but valid as a token
    INVALID,
    /// garbage after the parser has finished
    GARBAGE,
    /// Returned when a raw kind falls outside the enum; the parser never builds this
    THIS_SHOULD_NEVER_BE_EMITTED_GOD_DAMN_IT,
    // IMPORTANT! THIS NEEDS TO BE LAST!!!
    ROOT,
}

impl Token<'_> {
    fn kind(&self) -> SyntaxKind {
        match self.tpe() {
            TokenType::Eof => EOF,
            TokenType::Invalid => LX_INVALID,
            TokenType::Comment => LX_COMMENT,
            TokenType::Whitespace => LX_WHITESPACE,
            TokenType::Ident => LX_IDENT,
            TokenType::EscapedIdent => LX_ESCAPED_IDENT,
            TokenType::Div => LX_DIV,
            TokenType::Mul => LX_MUL,
            TokenType::Plus => LX_PLUS,
            TokenType::Minus => LX_MINUS,
            TokenType::Pipe => LX_PIPE,
            TokenType::DoubleColon => LX_DOUBLE_COLON,
            TokenType::Colon => LX_COLON,
            TokenType::Integer => LX_INTEGER,
            TokenType::Float => LX_FLOAT,
            TokenType::EqualEqual => LX_EQUAL_EQUAL,
            TokenType::Equal => LX_EQUAL,
            TokenType::Variable => LX_VARIABLE,
            TokenType::EscapedVariable => LX_ESCAPED_VARIABLE,
            TokenType::Regex => LX_REGEX,
            TokenType::Comma => LX_COMMA,
            TokenType::LParen => LX_L_PAREN,
            TokenType::RParen => LX_R_PAREN,
            TokenType::LBracket => LX_L_BRACKET,
            TokenType::RBracket => LX_R_BRACKET,
            TokenType::LBrace => LX_L_BRACE,
            TokenType::RBrace => LX_R_BRACE,
            TokenType::QuestionMark => LX_QUESTION_MARK,
            TokenType::Bang => LX_BANG,
            TokenType::SemiColon => LX_SEMI_COLON,
            TokenType::LessThanEqual => LX_LESS_THAN_EQUAL,
            TokenType::GreaterThanEqual => LX_GREATER_THAN_EQUAL,
            TokenType::LessThan => LX_LESS_THAN,
            TokenType::GreaterThan => LX_GREATER_THAN,
            TokenType::NotEqual => LX_NOT_EQUAL,
            TokenType::DotDot => LX_DOT_DOT,
            TokenType::String => LX_STRING,
            TokenType::StringSegment => LX_STRING_SEGMENT,
            TokenType::Bool => LX_BOOL,
            TokenType::Inf => LX_INF,
        }
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

#[allow(clippy::enum_glob_use)]
use SyntaxKind::*;

/// Parser for the MPL language syntax tree.
pub struct Parser<'input> {
    lexer: Peekable<Lexer<'input>>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
    eof: Token<'static>,
}

// Helper
impl<'input> Parser<'input> {
    /// Creates a new parser for the given input.
    #[must_use]
    pub fn new(input: &'input str) -> Self {
        Self {
            lexer: Lexer::new(input).peekable(),
            builder: GreenNodeBuilder::default(),
            errors: Vec::new(),
            eof: Token::new(TokenType::Eof, "", input.len()),
        }
    }

    fn eat_trivia(&mut self) {
        while let Some(token) = self.lexer.peek()
            && token.is_trivia()
        {
            let Some(token) = self.lexer.next() else {
                // we never reach the end of the input since we peeked above
                break;
            };
            self.token(token);
        }
    }

    fn token(&mut self, token: Token<'input>) {
        self.builder.token(token.kind().into(), token.text());
    }

    fn invalid(&mut self, token: Token<'input>) {
        self.builder.start_node(INVALID.into());
        self.token(token);
        self.builder.finish_node();
    }

    fn eat_token_type(&mut self, token_type: TokenType) {
        let tkn = self.next();
        if tkn.tpe() == token_type {
            self.token(tkn);
        } else {
            self.error_token(
                tkn,
                format!("expected {:?}, got {:?}", token_type, tkn.tpe()),
            );
        }
    }

    fn eat_token(&mut self) {
        let tkn = self.next();
        self.token(tkn);
    }

    fn structural(&mut self, token_type: TokenType) {
        self.eat_trivia();
        let token = self.next();
        if token.tpe() == token_type {
            self.token(token);
        } else {
            self.error_token(
                token,
                format!(
                    "expected structural {:?}, got {:?}",
                    token_type,
                    token.tpe()
                ),
            );
        }
        self.eat_trivia();
    }

    fn is_structural(&mut self, token_type: TokenType) -> bool {
        self.peek().tpe() == token_type
    }

    fn try_structural(&mut self, token_type: TokenType) -> bool {
        if !self.is_structural(token_type) {
            return false;
        }
        self.eat_token();
        true
    }

    fn is_keyword(&mut self, text: &str) -> bool {
        self.eat_trivia();
        let Some(token) = self.lexer.peek() else {
            return false;
        };
        token.tpe() == TokenType::Ident && token.text() == text
    }

    fn try_keyword(&mut self, text: &str) -> bool {
        if !self.is_keyword(text) {
            return false;
        }
        self.node(KEYWORD, |s| {
            s.eat_token();
        });
        true
    }

    fn peek(&mut self) -> Token<'input> {
        self.eat_trivia();
        if let Some(token) = self.lexer.peek() {
            *token
        } else {
            self.next();
            self.eof
        }
    }
    fn next(&mut self) -> Token<'input> {
        if let Some(token) = self.lexer.next() {
            token
        } else {
            self.eof();
            self.eof
        }
    }
    fn error(&mut self, message: impl Display) {
        // can't use peek as it would consume trivia
        if let Some(token) = self.lexer.peek()
            && token.tpe() != TokenType::Eof
        {
            let tkn = self.next();
            self.error_token(tkn, message);
        } else {
            self.error_token(self.eof, message);
        }
    }
    fn eof(&mut self) {
        if let Some(last) = self.errors.last()
            && matches!(last, SyntaxError::Eof { .. })
        {
            return;
        }
        self.errors.push(SyntaxError::Eof {
            range: SourceSpan::new(self.eof.pos().into(), 0),
        });
    }

    fn error_token(&mut self, token: Token<'input>, message: impl Display) {
        self.invalid(token);
        self.errors.push(SyntaxError::Generic {
            message: message.to_string(),
            range: SourceSpan::new(token.pos().into(), token.text().len()),
        });
    }

    fn node(&mut self, kind: SyntaxKind, f: impl FnOnce(&mut Self)) {
        self.builder.start_node(kind.into());
        self.eat_trivia();
        f(self);
        self.eat_trivia();
        self.builder.finish_node();
    }
}

/// Grammar
impl Parser<'_> {
    /// Parses the input and returns the syntax tree.
    #[must_use]
    pub fn parse(mut self) -> (SyntaxNode, Vec<SyntaxError>) {
        self.node(ROOT, |s| {
            s.eat_trivia();
            while s.is_keyword("set") {
                s.directive();
            }
            while s.is_keyword("param") {
                s.param();
            }

            s.query();
            let rest = s.peek();
            if rest.tpe() != TokenType::Eof {
                let mut last = rest;
                s.node(GARBAGE, |s| {
                    while let t = s.next()
                        && t.tpe() != TokenType::Eof
                    {
                        s.token(t);
                        last = t;
                    }
                });
                let start = rest.pos();
                let end = last.pos() + last.text().len();
                let len = end - start;
                s.errors.push(SyntaxError::TokenAfterEoq {
                    kind: rest.tpe(),
                    range: SourceSpan::new(start.into(), len),
                });
            }
        });
        (SyntaxNode::new_root(self.builder.finish()), self.errors)
    }

    fn query(&mut self) {
        self.node(QUERY, |s| {
            let token = s.peek();
            match token.tpe() {
                TokenType::Ident
                | TokenType::EscapedIdent
                | TokenType::Variable
                | TokenType::EscapedVariable => {
                    s.simple_query();
                }
                TokenType::LParen => {
                    s.compute_query();
                }
                _ => s.error("expected query"),
            }
        });
    }

    fn compute_query(&mut self) {
        self.node(COMPUTE_QUERY, |s| {
            s.structural(TokenType::LParen);
            s.query();
            s.structural(TokenType::Comma);
            s.query();
            // this is optional
            s.try_structural(TokenType::Comma);
            s.structural(TokenType::RParen);
            s.structural(TokenType::Pipe);
            s.keyword("compute");
            s.ident();
            s.keyword("using");
            let tkn = s.peek();
            match tkn.tpe() {
                TokenType::Ident | TokenType::EscapedIdent => s.function_path(),
                TokenType::Plus => s.eat_token_type(TokenType::Plus),
                TokenType::Minus => s.eat_token_type(TokenType::Minus),
                TokenType::Mul => s.eat_token_type(TokenType::Mul),
                TokenType::Div => s.eat_token_type(TokenType::Div),
                _ => s.error("expected compute function"),
            }
            s.rules();
        });
    }

    fn simple_query(&mut self) {
        self.node(SIMPLE_QUERY, |s| {
            s.ident_or_variable();
            s.structural(TokenType::Colon);
            s.ident();
            if s.peek().tpe() == TokenType::LBracket {
                s.time_range();
            }
            if s.try_keyword("as") {
                s.ident();
            }
            s.rules();
        });
    }

    fn time(&mut self) {
        self.node(TIME, Self::duration);
    }

    fn time_range(&mut self) {
        self.node(TIME_RANGE, |s| {
            s.structural(TokenType::LBracket);
            s.time();
            s.structural(TokenType::DotDot);
            if !s.try_structural(TokenType::RBracket) {
                s.time();
                s.structural(TokenType::RBracket);
            }
        });
    }

    fn directive(&mut self) {
        self.node(DIRECTIVE, |s| {
            s.keyword("set");
            s.ident();
            if s.try_structural(TokenType::Equal) {
                s.constant();
            }
            s.structural(TokenType::SemiColon);
        });
    }

    fn param(&mut self) {
        self.node(PARAM, |s| {
            s.keyword("param");
            s.variable();
            s.structural(TokenType::Colon);
            s.variable_type();
            s.structural(TokenType::SemiColon);
        });
    }

    fn float(&mut self) {
        self.node(FLOAT, |s| s.eat_token_type(TokenType::Float));
    }

    fn integer(&mut self) {
        self.node(INTEGER, |s| s.eat_token_type(TokenType::Integer));
    }

    fn bool(&mut self) {
        self.node(BOOL, |s| s.eat_token_type(TokenType::Bool));
    }

    fn constant(&mut self) {
        self.node(CONST, |s| {
            // we first consume all the + and - in the world
            while s.try_structural(TokenType::Plus) || s.try_structural(TokenType::Minus) {}
            let token = s.peek();
            match token.tpe() {
                TokenType::Inf => {
                    s.node(FLOAT, Parser::eat_token);
                }
                TokenType::Float => s.float(),
                TokenType::Integer => s.integer(),
                TokenType::Bool => s.bool(),
                TokenType::String | TokenType::StringSegment => s.string(),
                TokenType::LBracket => s.array(),
                _ => s.error("expected constant"),
            }
        });
    }

    fn expr(&mut self) {
        self.node(EXPR, |s| match s.peek().tpe() {
            TokenType::Ident | TokenType::EscapedIdent => {
                s.ident();
            }
            TokenType::Variable | TokenType::EscapedVariable => {
                s.variable();
            }
            _ => s.constant(),
        });
    }

    fn string(&mut self) {
        self.node(STRING, |s| {
            let mut tkn = s.next();
            while tkn.tpe() == TokenType::StringSegment {
                s.token(tkn);
                s.expr();
                tkn = s.next();
            }
            if tkn.tpe() == TokenType::String {
                s.token(tkn);
            } else {
                s.error_token(tkn, "Unexpected string");
            }
        });
    }

    fn array(&mut self) {
        self.node(ARRAY, |s| {
            s.structural(TokenType::LBracket);
            if s.try_structural(TokenType::RBracket) {
                return;
            }
            s.expr();
            while s.try_structural(TokenType::Comma) {
                s.expr();
            }
            s.structural(TokenType::RBracket);
        });
    }

    fn keyword(&mut self, text: &str) {
        self.node(KEYWORD, |s| {
            let token = s.next();
            if token.text() == text {
                s.token(token);
            } else {
                s.error_token(
                    token,
                    format!("expected keyword {} but got {}", text, token.text()),
                );
            }
        });
    }

    fn try_variable(&mut self) -> bool {
        if matches!(
            self.peek().tpe(),
            TokenType::Variable | TokenType::EscapedVariable
        ) {
            self.variable();
            true
        } else {
            false
        }
    }

    fn variable(&mut self) {
        self.node(VARIABLE, |s| {
            if s.peek().tpe() == TokenType::EscapedVariable {
                s.eat_token_type(TokenType::EscapedVariable);
            } else {
                s.eat_token_type(TokenType::Variable);
            }
        });
    }

    fn ident(&mut self) {
        self.node(IDENT, |s| {
            let token = s.next();
            if token.tpe() == TokenType::Ident || token.tpe() == TokenType::EscapedIdent {
                s.token(token);
            } else {
                s.error_token(
                    token,
                    format!(
                        "expected ident but got {} ({:?})",
                        token.text(),
                        token.tpe()
                    ),
                );
            }
        });
    }

    fn ident_or_variable(&mut self) {
        self.node(IDENT_OR_VARIABLE, |s| {
            let token = s.peek();
            match token.tpe() {
                TokenType::Ident | TokenType::EscapedIdent => s.ident(),
                TokenType::Variable | TokenType::EscapedVariable => s.variable(),
                _ => {
                    s.error_token(
                        token,
                        format!(
                            "expected ident or variable but got {} ({:?})",
                            token.text(),
                            token.tpe()
                        ),
                    );
                }
            }
        });
    }

    fn variable_type(&mut self) {
        self.node(TYPE, |s| {
            let token = s.next();
            if token.tpe() != TokenType::Ident {
                s.error_token(
                    token,
                    format!(
                        "expected variable type but got {} ({:?})",
                        token.text(),
                        token.tpe()
                    ),
                );
                return;
            }
            match token.text() {
             // built-in type
            "string" | "int" | "float" | "bool" | "array" |
            // custom type
            "Dataset" | "Duration" | "duration" | "Regex" =>
                s.token(token),
            "Option" => {
                s.token(token);
                s.structural(TokenType::LessThan);
                s.variable_type();
                s.structural(TokenType::GreaterThan);
            }
            _ => {
                s.error_token(
                    token,
                    format!("unknown type {}", token.text()),
                );
            }
        }
        });
    }

    fn rules(&mut self) {
        while self.try_structural(TokenType::Pipe) {
            self.node(RULE, |s| {
                let token = s.peek();
                let tpe = token.tpe();
                let txt = token.text();
                if tpe != TokenType::Ident {
                    s.error(format!("expected ident, got {tpe:?}"));
                    return;
                }
                match txt {
                    "filter" | "where" => s.filter_rule(),
                    "sample" => s.sample_rule(),
                    "map" => s.map_rule(),
                    "align" => s.align_rule(),
                    "group" => s.group_rule(),
                    "bucket" => s.bucket_rule(),
                    "ifdef" => s.ifdef_rule(),
                    "extend" => s.extend_rule(),
                    "as" => s.as_rule(),
                    _ => s.error(format!("unknown rule: {txt}")),
                }
            });
        }
    }

    fn filter_rule(&mut self) {
        self.node(FILTER, |s| {
            if !s.try_keyword("filter") && !s.try_keyword("where") {
                s.error("expected filter or where");
                return;
            }

            s.filter_or();
        });
    }
    fn filter_or(&mut self) {
        self.node(FILTER_OR, |s| {
            s.filter_and();
            while s.try_keyword("or") {
                s.filter_and();
            }
        });
    }

    fn filter_and(&mut self) {
        self.node(FILTER_AND, |s| {
            s.filter_not();
            while s.try_keyword("and") {
                s.filter_not();
            }
        });
    }

    fn filter_not(&mut self) {
        self.node(FILTER_NOT, |s| {
            s.try_keyword("not");
            s.filter_paren();
        });
    }
    fn filter_paren(&mut self) {
        self.node(FILTER_PAREN, |s| {
            if s.try_structural(TokenType::LParen) {
                s.filter_or();
                s.structural(TokenType::RParen);
            } else {
                s.filter_cmp();
            }
        });
    }
    fn regex(&mut self) {
        self.node(REGEX, |s| s.eat_token_type(TokenType::Regex));
    }
    fn filter_cmp(&mut self) {
        self.node(FILTER_CMP, |s| {
            s.ident();

            let tkn = s.peek();
            match tkn.tpe() {
                tt @ (TokenType::EqualEqual | TokenType::NotEqual) => {
                    s.structural(tt);
                    let tkn = s.peek();
                    if tkn.tpe() == TokenType::Regex {
                        s.regex();
                    } else {
                        s.expr();
                    }
                }

                tt @ (TokenType::LessThan
                | TokenType::GreaterThan
                | TokenType::LessThanEqual
                | TokenType::GreaterThanEqual) => {
                    s.structural(tt);
                    s.expr();
                }
                TokenType::Ident if tkn.text() == "is" => {
                    s.keyword("is");
                    s.type_ident();
                }
                TokenType::Ident if tkn.text() == "in" => {
                    s.keyword("in");
                    if !s.try_variable() {
                        s.array();
                    }
                }
                _ => {
                    s.error_token(tkn, "expected comparison operator");
                }
            }
        });
    }

    fn sample_rule(&mut self) {
        self.node(SAMPLE, |s| {
            s.keyword("sample");
            s.float();
        });
    }

    fn map_rule(&mut self) {
        self.node(MAP, |s| {
            s.keyword("map");
            let tkn = s.peek();
            match tkn.tpe() {
                TokenType::Mul | TokenType::Div | TokenType::Plus | TokenType::Minus => {
                    s.node(MAP_MATH, |s| {
                        s.eat_token();
                        s.expr();
                    });
                }
                _ => {
                    s.function_path();
                    if s.try_structural(TokenType::LParen) {
                        s.constant();
                        s.structural(TokenType::RParen);
                    }
                }
            }
        });
    }
    fn as_rule(&mut self) {
        self.node(AS, |s| {
            if !s.try_keyword("as") {
                s.error("expected as");
                return;
            }
            s.ident();
        });
    }

    fn align_rule(&mut self) {
        self.node(ALIGN, |s| {
            s.keyword("align");
            // note: this will eat "to $..." with the && as try_variable() is only
            // called when try_to is also true
            if s.try_keyword("to") && !s.try_variable() {
                s.duration();
            }
            s.keyword("using");
            s.function_path();
        });
    }

    fn duration(&mut self) {
        self.node(DURATION, |s| {
            let tkn = s.next();
            if tkn.tpe() != TokenType::Integer {
                s.error_token(tkn, "expected integer duration");
                return;
            }
            s.token(tkn);

            let tkn = s.peek();
            if tkn.tpe() == TokenType::Ident
                && matches!(tkn.text(), "ms" | "s" | "m" | "h" | "d" | "w" | "M" | "y")
            {
                s.node(TIME_UNIT, Parser::eat_token);
            }
        });
    }

    fn function_path(&mut self) {
        self.node(FUNCTION_PATH, |s| {
            s.ident();
            while s.try_structural(TokenType::DoubleColon) {
                s.ident();
            }
        });
    }

    fn group_rule(&mut self) {
        self.node(GROUP, |s| {
            s.keyword("group");
            if s.try_keyword("by") {
                s.tag_list();
            }
            s.keyword("using");
            s.function_path();
        });
    }

    fn bucket_rule(&mut self) {
        self.node(BUCKET, |s| {
            s.keyword("bucket");
            if s.try_keyword("by") {
                s.tag_list();
            }
            if s.try_keyword("to") && !s.try_variable() {
                s.duration();
            }
            s.keyword("using");
            s.function_path();
            s.structural(TokenType::LParen);
            if !s.is_structural(TokenType::RParen) {
                s.bucket_args();
            }
            s.structural(TokenType::RParen);
        });
    }

    fn bucket_arg(&mut self) {
        self.node(BUCKET_ARG, |s| {
            let tkn = s.peek();
            match tkn.tpe() {
                TokenType::Ident | TokenType::EscapedIdent => {
                    s.ident();
                }
                TokenType::Float => {
                    s.float();
                }
                _ => {
                    s.error_token(tkn, "expected ident or float in bucket arg");
                }
            }
        });
    }

    fn bucket_args(&mut self) {
        self.node(BUCKET_ARGS, |s| {
            s.bucket_arg();
            while s.try_structural(TokenType::Comma) {
                s.bucket_arg();
            }
        });
    }

    fn ifdef_rule(&mut self) {
        self.node(IFDEF, |s| {
            s.keyword("ifdef");
            s.structural(TokenType::LParen);
            s.variable();
            s.structural(TokenType::RParen);
            s.structural(TokenType::LBrace);
            s.filter_rule();
            s.structural(TokenType::RBrace);
            if s.try_keyword("else") {
                s.structural(TokenType::LBrace);
                s.filter_rule();
                s.structural(TokenType::RBrace);
            }
        });
    }

    fn extend_part(&mut self) {
        self.node(EXTEND_PART, |s| {
            s.ident();
            s.structural(TokenType::Equal);
            s.expr();
        });
    }

    fn extend_rule(&mut self) {
        self.node(EXTEND, |s| {
            s.keyword("extend");
            s.extend_part();
            while s.try_structural(TokenType::Comma) {
                s.extend_part();
            }
        });
    }

    fn tag_list(&mut self) {
        self.node(TAG_LIST, |s| {
            s.ident();
            while s.try_structural(TokenType::Comma) {
                s.ident();
            }
        });
    }

    fn type_ident(&mut self) {
        self.node(OTEL_TYPE, |s| {
            let tkn = s.peek();
            if tkn.tpe() == TokenType::Ident
                && matches!(tkn.text(), "string" | "int" | "float" | "bool" | "array")
            {
                s.ident();
            } else {
                s.error_token(tkn, "expected otel type ident");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse() {
        let input = r"
            // test
            set a = 42;
            set b;
            a:b
            ";
        let (tree, errors) = Parser::new(input).parse();
        dbg!(&tree, &errors);
        assert_eq!(input, tree.to_string());

        assert!(errors.is_empty());
    }
    #[test]
    fn syntax_type_array() {
        assert_eq!(ALL_KINDS.len(), ROOT as usize + 1);
        for (i, kind) in ALL_KINDS.iter().enumerate() {
            assert_eq!(*kind as usize, i);
        }
    }
}
