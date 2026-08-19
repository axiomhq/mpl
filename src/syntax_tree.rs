use std::fmt::Display;

use itertools::{PeekNth, peek_nth};
use miette::{Diagnostic, MietteDiagnostic, SourceSpan};
use rowan::GreenNodeBuilder;
use strum::VariantArray;

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
impl SyntaxError {
    /// Converts this error into a [`MietteDiagnostic`].
    pub fn to_diagnostic(&self) -> MietteDiagnostic {
        MietteDiagnostic {
            message: self.to_string(),
            code: self.code().map(|code| code.to_string()),
            severity: self.severity(),
            help: self.help().map(|help| help.to_string()),
            url: self.url().map(|url| url.to_string()),
            labels: self.labels().map(Iterator::collect),
        }
    }
}
/// The language definition for the MPL language syntax tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Lang {}
impl rowan::Language for Lang {
    type Kind = SyntaxKind;
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        *SyntaxKind::VARIANTS
            .get(raw.0 as usize)
            .unwrap_or(&THIS_SHOULD_NEVER_BE_EMITTED_GOD_DAMN_IT)
    }
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// A syntax node in the MPL language syntax tree.
pub type SyntaxNode = rowan::SyntaxNode<Lang>;

/// The syntax kind of a node in the MPL language syntax tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, VariantArray)]
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
    LX_STRING_START,
    LX_STRING_SEGMENT,
    LX_STRING_END,
    LX_BOOL,
    LX_NULL,
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
    FILTER_CMP_EQ,
    FILTER_CMP_NEQ,
    FILTER_CMP_LT,
    FILTER_CMP_GT,
    FILTER_CMP_LTE,
    FILTER_CMP_GTE,
    FILTER_CMP_IN,
    FILTER_CMP_IS,
    FUNCTION_CALL,
    FUNCTION_PATH,
    FUNCTION_ARGS,
    MATH_FN,
    DURATION,
    TIME_UNIT,
    OTEL_TYPE,
    MPL_TYPE,
    OPTION_TYPE,

    EXPR,
    REGEX,
    CONST,
    INTEGER,
    FLOAT,
    BOOL,
    NULL,
    STRING,
    STRING_SEGMENT,
    STRING_END,
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
    MAP_MUL,
    MAP_DIV,
    MAP_PLUS,
    MAP_MINUS,
    ALIGN,
    AS,
    GROUP,
    BUCKET,

    /// invalid in the syntax tree but valid as a token
    INVALID,
    /// garbage after the parser has finished
    GARBAGE,
    /// Returned when a raw kind falls outside the enum; the parser never builds this
    THIS_SHOULD_NEVER_BE_EMITTED_GOD_DAMN_IT,
    // IMPORTANT! THIS NEEDS TO BE LAST!!!
    ROOT,
}
#[allow(clippy::enum_glob_use)]
use SyntaxKind::*;

impl SyntaxKind {
    /// Returns `true` if the kind is a trivia token (comment, whitespace, or invalid).
    #[must_use]
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            SyntaxKind::LX_COMMENT
                | SyntaxKind::LX_WHITESPACE
                | SyntaxKind::LX_INVALID
                | SyntaxKind::LX_SEMI_COLON
                | SyntaxKind::INVALID
                | SyntaxKind::GARBAGE
                | SyntaxKind::THIS_SHOULD_NEVER_BE_EMITTED_GOD_DAMN_IT
                | SyntaxKind::ROOT
        )
    }
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
            TokenType::StringStart => LX_STRING_START,
            TokenType::StringSegment => LX_STRING_SEGMENT,
            TokenType::StringEnd => LX_STRING_END,
            TokenType::Bool => LX_BOOL,
            TokenType::Null => LX_NULL,
            TokenType::Inf => LX_INF,
        }
    }
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

/// Parser for the MPL language syntax tree.
pub struct Parser<'input> {
    lexer: PeekNth<Lexer<'input>>,
    builder: GreenNodeBuilder<'static>,
    errors: Vec<SyntaxError>,
    eof: Token<'static>,
    max_tree_depth: usize,
    depth: usize,
}

// Helper
impl<'input> Parser<'input> {
    /// Creates a new parser for the given input.
    #[must_use]
    pub fn new(input: &'input str) -> Self {
        Self {
            lexer: peek_nth(Lexer::new(input)),
            builder: GreenNodeBuilder::default(),
            errors: Vec::new(),
            eof: Token::new(TokenType::Eof, "", input.len()),
            max_tree_depth: 250,
            depth: 0,
        }
    }
    /// Limits the tree depth of the parser. (and in effect the recursion)
    /// default is 250
    #[must_use]
    pub fn with_tree_depth(mut self, depth: usize) -> Self {
        self.max_tree_depth = depth;
        self
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

    /// returns the nth non-trivia token
    fn peek_nth(&mut self, mut n: usize) -> Option<Token<'input>> {
        let mut i = 0;
        while let Some(t) = self.lexer.peek_nth(i) {
            i += 1;
            if t.is_trivia() {
                continue;
            }
            if n == 0 {
                return Some(*t);
            }
            n -= 1;
        }
        None
    }
    /// returns the nth non-trivia token type
    fn peek_nth_type(&mut self, n: usize) -> Option<TokenType> {
        self.peek_nth(n).map(|t| t.tpe())
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
    /// silent keyword ; does not produce a new syntax node just a token
    fn try_keyword_token(&mut self, text: &str) -> bool {
        if !self.is_keyword(text) {
            return false;
        }
        self.eat_token();
        true
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

    fn garbage_tail(&mut self) {
        let rest = self.peek();
        if rest.tpe() == TokenType::Eof {
            return;
        }
        let mut last = rest;
        self.node(GARBAGE, |s| {
            while let t = s.next()
                && t.tpe() != TokenType::Eof
            {
                s.token(t);
                last = t;
            }
        });
        let start = rest.pos();
        let end = last.pos() + last.text().len();
        self.errors.push(SyntaxError::TokenAfterEoq {
            kind: rest.tpe(),
            range: SourceSpan::new(start.into(), end - start),
        });
    }

    fn finish(self) -> SyntaxTree {
        SyntaxTree {
            root: SyntaxNode::new_root(self.builder.finish()),
            errors: self.errors,
        }
    }

    fn node(&mut self, kind: SyntaxKind, f: impl FnOnce(&mut Self)) {
        self.builder.start_node(kind.into());
        self.eat_trivia();
        f(self);
        self.builder.finish_node();
        self.eat_trivia();
    }
    fn rnode(&mut self, kind: SyntaxKind, f: impl FnOnce(&mut Self)) {
        self.depth += 1;
        if self.depth > self.max_tree_depth {
            self.error("recursion depth exceeded");
        } else {
            self.node(kind, f);
        }
        self.depth = self.depth.saturating_sub(1);
    }
}

/// Represents a parsed syntax tree.
pub struct SyntaxTree {
    /// root node
    pub root: SyntaxNode,
    /// errors encountered during parsing
    pub errors: Vec<SyntaxError>,
}
/// Grammar
impl Parser<'_> {
    /// Parses the input and returns the syntax tree.
    #[must_use]
    pub fn parse(mut self) -> SyntaxTree {
        self.node(ROOT, |s| {
            s.eat_trivia();
            while s.is_keyword("set") {
                s.directive();
            }
            while s.is_keyword("param") {
                s.param();
            }

            s.query();
            s.garbage_tail();
        });
        self.finish()
    }

    #[must_use]
    pub(crate) fn parse_ident_value(self) -> SyntaxTree {
        self.single_value(Self::ident)
    }

    #[must_use]
    pub(crate) fn parse_duration_value(self) -> SyntaxTree {
        self.single_value(Self::duration)
    }

    #[must_use]
    pub(crate) fn parse_regex_value(self) -> SyntaxTree {
        self.single_value(Self::regex)
    }

    #[must_use]
    pub(crate) fn parse_const_value(self) -> SyntaxTree {
        self.single_value(Self::constant)
    }

    fn single_value(mut self, val: fn(&mut Self)) -> SyntaxTree {
        self.node(ROOT, |s| {
            val(s);
            s.garbage_tail();
        });
        self.finish()
    }

    fn query(&mut self) {
        self.rnode(QUERY, |s| {
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
            s.keyword_token("compute");
            s.ident();
            s.keyword_token("using");
            let tkn = s.peek();
            match tkn.tpe() {
                TokenType::Ident | TokenType::EscapedIdent => s.function_call(),
                TokenType::Plus => s.node(FUNCTION_CALL, |s| {
                    s.node(MATH_FN, |s| {
                        s.eat_token_type(TokenType::Plus);
                    });
                    s.node(FUNCTION_ARGS, |_| {});
                }),
                TokenType::Minus => s.node(FUNCTION_CALL, |s| {
                    s.node(MATH_FN, |s| {
                        s.eat_token_type(TokenType::Minus);
                    });
                    s.node(FUNCTION_ARGS, |_| {});
                }),
                TokenType::Mul => s.node(FUNCTION_CALL, |s| {
                    s.node(MATH_FN, |s| {
                        s.eat_token_type(TokenType::Mul);
                    });
                    s.node(FUNCTION_ARGS, |_| {});
                }),
                TokenType::Div => s.node(FUNCTION_CALL, |s| {
                    s.node(MATH_FN, |s| {
                        s.eat_token_type(TokenType::Div);
                    });
                    s.node(FUNCTION_ARGS, |_| {});
                }),
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
            s.keyword_token("set");
            s.ident();
            if s.try_structural(TokenType::Equal) {
                s.constant();
            }
            s.structural(TokenType::SemiColon);
        });
    }

    fn param(&mut self) {
        self.node(PARAM, |s| {
            s.keyword_token("param");
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
    fn null(&mut self) {
        self.node(NULL, |s| s.eat_token_type(TokenType::Null));
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
                TokenType::Null => s.null(),
                TokenType::String | TokenType::StringStart => s.string(),
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
        self.rnode(STRING, |s| {
            let mut tkn = s.next();
            if tkn.tpe() == TokenType::StringStart {
                s.token(tkn);
                s.expr();
                tkn = s.next();
                while tkn.tpe() == TokenType::StringSegment {
                    s.token(tkn);
                    s.expr();
                    tkn = s.next();
                }
                if tkn.tpe() == TokenType::StringEnd {
                    s.token(tkn);
                } else {
                    s.error_token(tkn, "Unexpected string");
                }
            } else if tkn.tpe() == TokenType::String {
                s.token(tkn);
            } else {
                s.error_token(tkn, "Unexpected string");
            }
        });
    }

    fn array(&mut self) {
        self.rnode(ARRAY, |s| {
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

    /// silent keyword ; does not produce a new syntax node just a token
    fn keyword_token(&mut self, text: &str) {
        let token = self.next();
        if token.text() == text {
            self.token(token);
        } else {
            self.error_token(
                token,
                format!("expected keyword {} but got {}", text, token.text()),
            );
        }
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
            let token = s.peek();
            if token.tpe() != TokenType::Ident {
                s.error(format!(
                    "expected variable type but got {} ({:?})",
                    token.text(),
                    token.tpe()
                ));
                return;
            }
            match token.text() {
                // built-in type
                "string" | "int" | "float" | "bool" | "array" | "null" => {
                    s.node(OTEL_TYPE, Parser::eat_token);
                }
                // custom type
                "Dataset" | "Duration" | "Regex" | "Timestamp" => {
                    s.node(MPL_TYPE, Parser::eat_token);
                }
                "Option" => s.rnode(OPTION_TYPE, |s| {
                    s.eat_token();
                    s.structural(TokenType::LessThan);
                    s.variable_type();
                    s.structural(TokenType::GreaterThan);
                }),
                _ => {
                    s.error(format!("unknown type {}", token.text()));
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
            if !s.try_keyword_token("filter") && !s.try_keyword_token("where") {
                s.error("expected filter or where");
                return;
            }

            s.filter_or();
        });
    }
    fn filter_or(&mut self) {
        self.rnode(FILTER_OR, |s| {
            s.filter_and();
            while s.try_keyword_token("or") {
                s.filter_and();
            }
        });
    }

    fn filter_and(&mut self) {
        self.rnode(FILTER_AND, |s| {
            s.filter_not();
            while s.try_keyword_token("and") {
                s.filter_not();
            }
        });
    }

    fn filter_not(&mut self) {
        self.rnode(FILTER_NOT, |s| {
            s.try_keyword("not");
            s.filter_paren();
        });
    }
    fn filter_paren(&mut self) {
        self.rnode(FILTER_PAREN, |s| {
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
        self.rnode(FILTER_CMP, |s| {
            s.ident();

            let tkn = s.peek();
            match tkn.tpe() {
                TokenType::EqualEqual => s.node(FILTER_CMP_EQ, |s| {
                    s.structural(TokenType::EqualEqual);
                    let tkn = s.peek();
                    if tkn.tpe() == TokenType::Regex {
                        s.regex();
                    } else {
                        s.expr();
                    }
                }),
                TokenType::NotEqual => s.node(FILTER_CMP_NEQ, |s| {
                    s.structural(TokenType::NotEqual);
                    let tkn = s.peek();
                    if tkn.tpe() == TokenType::Regex {
                        s.regex();
                    } else {
                        s.expr();
                    }
                }),
                TokenType::LessThan => s.node(FILTER_CMP_LT, |s| {
                    s.structural(TokenType::LessThan);
                    s.expr();
                }),
                TokenType::GreaterThan => s.node(FILTER_CMP_GT, |s| {
                    s.structural(TokenType::GreaterThan);
                    s.expr();
                }),
                TokenType::LessThanEqual => s.node(FILTER_CMP_LTE, |s| {
                    s.structural(TokenType::LessThanEqual);
                    s.expr();
                }),
                TokenType::GreaterThanEqual => s.node(FILTER_CMP_GTE, |s| {
                    s.structural(TokenType::GreaterThanEqual);
                    s.expr();
                }),

                TokenType::Ident if tkn.text() == "is" => s.node(FILTER_CMP_IS, |s| {
                    s.keyword_token("is");
                    s.type_ident();
                }),
                TokenType::Ident if tkn.text() == "in" => s.node(FILTER_CMP_IN, |s| {
                    s.keyword_token("in");
                    if !s.try_variable() {
                        s.expr();
                    }
                }),
                _ => {
                    s.error("expected comparison operator");
                }
            }
        });
    }

    fn sample_rule(&mut self) {
        self.node(SAMPLE, |s| {
            s.keyword_token("sample");
            s.float();
        });
    }

    fn map_rule(&mut self) {
        self.node(MAP, |s| {
            s.keyword_token("map");
            let tkn = s.peek();
            match tkn.tpe() {
                TokenType::Mul => {
                    s.node(MAP_MUL, |s| {
                        s.eat_token();
                        s.expr();
                    });
                }
                TokenType::Div => {
                    s.node(MAP_DIV, |s| {
                        s.eat_token();
                        s.expr();
                    });
                }
                TokenType::Plus => {
                    s.node(MAP_PLUS, |s| {
                        s.eat_token();
                        s.expr();
                    });
                }
                TokenType::Minus => {
                    s.node(MAP_MINUS, |s| {
                        s.eat_token();
                        s.expr();
                    });
                }
                _ => {
                    s.function_call();
                }
            }
        });
    }
    fn as_rule(&mut self) {
        self.node(AS, |s| {
            if !s.try_keyword_token("as") {
                s.error("expected as");
                return;
            }
            s.ident();
        });
    }

    fn align_rule(&mut self) {
        self.node(ALIGN, |s| {
            s.keyword_token("align");
            // note: this will eat "to $..." with the && as try_variable() is only
            // called when try_to is also true
            if s.try_keyword("to") && !s.try_variable() {
                s.duration();
            }
            s.keyword("using");
            s.function_call();
        });
    }

    fn duration(&mut self) {
        self.node(DURATION, |s| {
            s.integer();
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
            s.keyword_token("group");
            if s.try_keyword("by") {
                s.tag_list();
            }
            s.keyword("using");
            s.function_call();
        });
    }

    fn bucket_rule(&mut self) {
        self.node(BUCKET, |s| {
            s.keyword_token("bucket");
            if s.try_keyword("by") {
                s.tag_list();
            }
            if s.try_keyword("to") && !s.try_variable() {
                s.duration();
            }
            s.keyword("using");
            s.function_call();
        });
    }

    fn ifdef_rule(&mut self) {
        self.node(IFDEF, |s| {
            s.keyword_token("ifdef");
            s.structural(TokenType::LParen);
            s.variable();
            s.structural(TokenType::RParen);
            s.structural(TokenType::LBrace);
            s.filter_rule();
            s.structural(TokenType::RBrace);
            if s.try_keyword_token("else") {
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
            s.keyword_token("extend");
            s.extend_part();

            // the continuation here is `, <ident> = `
            // while `, <ident>:` or `, )` are not valid continuations
            // so we need to look ahead two tokens
            while s.peek_nth_type(0) == Some(TokenType::Comma)
                && s.peek_nth(1).is_some_and(|t| t.is_ident())
                && s.peek_nth_type(2) == Some(TokenType::Equal)
            {
                s.eat_token_type(TokenType::Comma);
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
                && matches!(
                    tkn.text(),
                    "string" | "int" | "float" | "bool" | "array" | "null"
                )
            {
                s.ident();
            } else {
                s.error("expected otel type ident");
            }
        });
    }

    fn function_call(&mut self) {
        self.node(FUNCTION_CALL, |s| {
            s.function_path();
            s.node(FUNCTION_ARGS, |s| {
                if s.try_structural(TokenType::LParen) && !s.try_structural(TokenType::RParen) {
                    s.expr();
                    while s.try_structural(TokenType::Comma) {
                        s.expr();
                    }
                    s.structural(TokenType::RParen);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_syntaxparse() {
        let input = r#"
            // test
            set a = 42;
            set b;
            a:b
            | where a == "hello ${ $world } snot { $badger }"
            "#;
        let SyntaxTree { root, errors } = Parser::new(input).parse();
        dbg!(&root, &errors);
        assert_eq!(input, root.to_string());

        assert!(errors.is_empty());
    }
}
