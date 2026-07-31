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
    Eof,
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
        assert!(raw.0 <= ROOT as u16, "invalid syntax kind: {}", raw.0);
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

/// A syntax node in the MPL language syntax tree.
pub type SyntaxNode = rowan::SyntaxNode<Lang>;

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
    FUNCION_PATH,
    DURATION,
    TIME_UNIT,

    EXPR,
    CONST,
    INTEGER,
    FLOAT,
    BOOL,
    STRING,
    ARRAY,

    RULE,
    FILTER,
    SAMPLE,
    MAP,
    ALIGN,

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
    // const ERROR: SyntaxKind = SyntaxKind(6666);

    /// Creates a new parser for the given input.
    #[must_use]
    pub fn new(input: &'input str) -> Self {
        Self {
            lexer: Lexer::new(input).peekable(),
            builder: GreenNodeBuilder::default(),
            errors: Vec::new(),
            eof: Token::new(TokenType::Invalid, "", input.len()),
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
    fn current<'a>(&'a mut self) -> Option<&'a Token<'input>> {
        self.lexer.peek()
    }

    fn token(&mut self, token: Token<'input>) {
        self.builder.token(token.kind().into(), token.text());
    }

    fn eat_token(&mut self) {
        let tkn = self.next();
        self.token(tkn);
    }

    fn structural(&mut self, token_type: TokenType) {
        self.eat_trivia();
        let token = self.next();
        if token.tpe() != token_type {
            self.error_token(
                token,
                format!(
                    "expected structural {:?}, got {:?}",
                    token_type,
                    token.tpe()
                ),
            );
        }
        self.token(token);
        self.eat_trivia();
    }

    fn is_structural(&mut self, token_type: TokenType) -> bool {
        let Some(token) = self.current() else {
            return false;
        };
        token.tpe() == token_type
    }

    fn is_keyword(&mut self, text: &str) -> bool {
        let Some(token) = self.lexer.peek() else {
            return false;
        };
        token.tpe() == TokenType::Ident && token.text() == text
    }

    fn peek(&mut self) -> Token<'input> {
        if let Some(token) = self.lexer.peek() {
            *token
        } else {
            self.errors.push(SyntaxError::Eof);
            self.eof
        }
    }
    fn next(&mut self) -> Token<'input> {
        if let Some(token) = self.lexer.next() {
            token
        } else {
            self.errors.push(SyntaxError::Eof);
            self.eof
        }
    }
    fn error(&mut self, message: impl Display) {
        let tkn = self.peek();
        self.error_token(tkn, message);
    }
    fn error_token(&mut self, token: Token<'input>, message: impl Display) {
        self.errors.push(SyntaxError::Generic {
            message: message.to_string(),
            range: SourceSpan::new(token.pos().into(), token.text().len()),
        });
    }
}

/// Grammer
impl Parser<'_> {
    /// Parses the input and returns the syntax tree.
    /// # Panics
    /// Panics because it's not done yet
    #[must_use]
    pub fn parse(mut self) -> (SyntaxNode, Vec<SyntaxError>) {
        self.start_node(ROOT);
        self.eat_trivia();
        while self.is_keyword("set") {
            self.directive();
        }
        while self.is_keyword("param") {
            self.param();
        }

        self.query();
        let rest = self.next();
        if rest.tpe() != TokenType::Eof {
            self.errors.push(SyntaxError::TokenAfterEoq {
                kind: rest.tpe(),
                range: SourceSpan::new(rest.pos().into(), rest.text().len()),
            });
        }
        self.finish_node();
        (SyntaxNode::new_root(self.builder.finish()), self.errors)
    }

    fn query(&mut self) {
        self.start_node(QUERY);
        let token = self.peek();
        match token.tpe() {
            TokenType::Ident | TokenType::EscapedIdent => {
                self.simple_query();
            }
            TokenType::LParen => {
                self.compute_query();
            }
            _ => self.error("expected query"),
        }
        self.finish_node();
    }

    fn compute_query(&mut self) {
        self.start_node(COMPUTE_QUERY);
        self.structural(TokenType::LParen);
        self.simple_query();
        self.structural(TokenType::Comma);
        self.simple_query();
        self.structural(TokenType::RParen);

        self.finish_node();
    }

    fn simple_query(&mut self) {
        self.start_node(SIMPLE_QUERY);
        self.ident_or_variable();
        self.structural(TokenType::Colon);
        self.ident();
        self.rules();
        self.finish_node();
    }

    fn directive(&mut self) {
        self.start_node(DIRECTIVE);
        self.keyword("set");
        self.ident();
        if self.is_structural(TokenType::Equal) {
            self.structural(TokenType::Equal);
            self.constant();
        }

        self.structural(TokenType::SemiColon);
        self.finish_node();
    }

    fn param(&mut self) {
        self.start_node(DIRECTIVE);
        self.keyword("param");
        self.variable();
        self.structural(TokenType::Equal);
        self.variable_type();
        self.structural(TokenType::SemiColon);
        self.finish_node();
    }

    fn constant(&mut self) {
        self.start_node(CONST);
        let token = self.peek();
        match token.tpe() {
            TokenType::Inf | TokenType::Float => {
                self.start_node(FLOAT);
                self.eat_token();
                self.finish_node();
            }
            TokenType::Integer => {
                self.start_node(INTEGER);
                self.eat_token();
                self.finish_node();
            }
            TokenType::Bool => {
                self.start_node(BOOL);
                self.eat_token();
                self.finish_node();
            }
            TokenType::String | TokenType::StringSegment => {
                self.string();
            }
            TokenType::LBrace => {
                self.array();
            }
            _ => self.error("expected value"),
        }

        self.finish_node();
    }

    fn expr(&mut self) {
        self.start_node(EXPR);
        match self.peek().tpe() {
            TokenType::Ident | TokenType::EscapedIdent => {
                self.ident();
            }
            TokenType::Variable | TokenType::EscapedVariable => {
                self.variable();
            }
            _ => self.constant(),
        }
        self.finish_node();
    }

    fn string(&mut self) {
        self.start_node(STRING);
        let mut tkn = self.next();
        while tkn.tpe() == TokenType::StringSegment {
            self.token(tkn);
            self.expr();
            tkn = self.next();
        }
        self.token(tkn);
        self.finish_node();
    }

    fn array(&mut self) {
        self.start_node(ARRAY);
        self.error("array is not implemented");
        self.finish_node();
    }

    fn keyword(&mut self, text: &str) {
        self.start_node(KEYWORD);
        let token = self.next();
        if token.text() != text {
            self.error_token(
                token,
                format!("expected keyword {} but got {}", text, token.text()),
            );
            self.finish_node();
            return;
        }
        self.token(token);
        self.finish_node();
    }

    fn try_keyword(&mut self, text: &str) -> bool {
        if !self.is_keyword(text) {
            return false;
        }
        self.start_node(KEYWORD);
        self.eat_token();
        self.finish_node();
        true
    }

    fn variable(&mut self) {
        self.start_node(VARIABLE);
        let token = self.next();
        if token.tpe() != TokenType::Variable {
            self.error_token(
                token,
                format!(
                    "expected variable but got {} ({:?})",
                    token.text(),
                    token.tpe()
                ),
            );
            self.finish_node();
            return;
        }
        self.token(token);
        self.finish_node();
    }

    fn ident(&mut self) {
        self.start_node(IDENT);
        let token = self.next();
        if token.tpe() != TokenType::Ident && token.tpe() != TokenType::EscapedIdent {
            self.error_token(
                token,
                format!(
                    "expected ident but got {} ({:?})",
                    token.text(),
                    token.tpe()
                ),
            );
            self.finish_node();
            return;
        }
        self.token(token);
        self.finish_node();
    }

    fn ident_or_variable(&mut self) {
        self.start_node(IDENT_OR_VARIABLE);

        let token = self.peek();
        match token.tpe() {
            TokenType::Ident | TokenType::EscapedIdent => self.ident(),
            TokenType::Variable | TokenType::EscapedVariable => self.variable(),
            _ => {
                self.error_token(
                    token,
                    format!(
                        "expected ident or variable but got {} ({:?})",
                        token.text(),
                        token.tpe()
                    ),
                );
            }
        }
        self.finish_node();
    }

    fn variable_type(&mut self) {
        self.start_node(TYPE);
        let token = self.next();
        if token.tpe() != TokenType::Ident {
            self.error_token(
                token,
                format!(
                    "expected variable type but got {} ({:?})",
                    token.text(),
                    token.tpe()
                ),
            );
            self.finish_node();
            return;
        }
        match token.text() {
             // built-in type
            "string" | "int" | "float" | "bool" | "array" |
            // custom type
            "Dataset" | "Duration" | "duration" | "Regex" =>
                self.token(token),
            "Option" => {
                self.token(token);
                self.structural(TokenType::LessThan);
                self.variable_type();
                self.structural(TokenType::GreaterThan);
            }
            _ => {
                self.error_token(
                    token,
                    format!("unknown type {}", token.text()),
                );
            }
        }
        self.finish_node();
    }

    fn start_node(&mut self, kind: SyntaxKind) {
        self.builder.start_node(kind.into());
        self.eat_trivia();
    }
    fn finish_node(&mut self) {
        self.eat_trivia();
        self.builder.finish_node();
    }

    fn rules(&mut self) {
        while self.is_structural(TokenType::Pipe) {
            self.start_node(RULE);
            self.structural(TokenType::Pipe);
            let token = self.peek();
            let tpe = token.tpe();
            let txt = token.text();
            if tpe != TokenType::Ident {
                self.error(format!("expected ident, got {tpe:?}"));
            }
            match txt {
                "filter" | "where" => self.filter_rule(),
                "sample" => self.sample_rule(),
                "map" => self.map_rule(),
                "align" => self.align_rule(),
                "group" => self.group_rule(),
                "bucket" => self.bucket_rule(),
                "ifdef" => self.ifdef_rule(),
                "extend" => self.extend_rule(),
                _ => self.error(format!("unknown rule: {txt}")),
            }
            self.finish_node();
        }
    }

    fn filter_rule(&mut self) {
        self.start_node(FILTER);
        if !self.try_keyword("filter") && !self.try_keyword("where") {
            self.error("expected filter or where");
            self.finish_node();
            return;
        }

        self.filter_or();

        self.finish_node();
    }
    fn filter_or(&mut self) {
        self.start_node(FILTER_OR);
        self.filter_and();
        if self.try_keyword("or") {
            self.filter_and();
        }
        self.finish_node();
    }

    fn filter_and(&mut self) {
        self.start_node(FILTER_AND);
        self.filter_not();
        if self.try_keyword("and") {
            self.filter_not();
        }
        self.finish_node();
    }

    fn filter_not(&mut self) {
        self.start_node(FILTER_NOT);
        self.try_keyword("not");
        self.filter_paren();
        self.finish_node();
    }
    fn filter_paren(&mut self) {
        self.start_node(FILTER_PAREN);
        if self.is_structural(TokenType::LParen) {
            self.eat_token();
            self.filter_or();
            self.structural(TokenType::RParen);
        } else {
            self.filter_cmp();
        }
        self.finish_node();
    }
    fn filter_cmp(&mut self) {
        self.start_node(FILTER_CMP);
        self.ident();

        let tkn = self.peek();
        match tkn.tpe() {
            tt @ (TokenType::EqualEqual
            | TokenType::NotEqual
            | TokenType::LessThan
            | TokenType::GreaterThan
            | TokenType::LessThanEqual
            | TokenType::GreaterThanEqual) => self.structural(tt),
            TokenType::Ident if tkn.text() == "is" => self.keyword("is"),
            TokenType::Ident if tkn.text() == "in" => self.keyword("in"),
            _ => self.error_token(tkn, "expected comparison operator"),
        }

        self.constant();
        self.finish_node();
    }

    fn sample_rule(&mut self) {
        self.start_node(SAMPLE);
        self.keyword("sample");
        self.constant(); // TODO: this should be float?
        self.finish_node();
    }

    fn map_rule(&mut self) {
        self.start_node(MAP);
        self.keyword("map");
        self.ident();
        if self.is_structural(TokenType::LParen) {
            self.eat_token();
            self.constant();
            self.structural(TokenType::RParen);
        }
        self.finish_node();
    }

    fn align_rule(&mut self) {
        self.start_node(ALIGN);
        self.keyword("align");
        if self.try_keyword("to") {
            self.duration();
        }
        self.keyword("using");
        self.funcion_path();
        self.finish_node();
    }

    fn duration(&mut self) {
        self.start_node(DURATION);

        let tkn = self.next();
        if tkn.tpe() != TokenType::Integer {
            self.error_token(tkn, "expected integer duration");
        }
        self.token(tkn);

        let tkn = self.peek();
        if tkn.tpe() == TokenType::Ident
            && matches!(tkn.text(), "ms" | "s" | "m" | "h" | "d" | "w" | "M" | "y")
        {
            self.start_node(TIME_UNIT);
            self.eat_token();
            self.finish_node();
        }
        self.token(tkn);

        self.finish_node();
    }

    fn funcion_path(&mut self) {
        self.start_node(FUNCION_PATH);
        self.ident();
        while self.is_structural(TokenType::DoubleColon) {
            self.eat_token();
            self.ident();
        }
    }

    fn group_rule(&self) {
        todo!()
    }

    fn bucket_rule(&self) {
        todo!()
    }

    fn ifdef_rule(&self) {
        todo!()
    }

    fn extend_rule(&self) {
        todo!()
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
}
