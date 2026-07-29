// Choices:
//
// - we do not allow unicode escapes
// - we do not allow rfc 3339 timestamps
//
use std::{iter::Peekable, str::Chars};

/// Represents a token parsed from the input, type, position in the input and text representation.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Token<'input> {
    /// The type of the token.
    tpe: TokenType,
    /// The text of the token.
    text: &'input str,
    /// The byte position of the token in the input string.
    pos: usize,
}

/// Represents the type of a token.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TokenType {
    /// An invalid token.
    Invalid,
    /// Whitespace.
    Whitespace,
    /// An identifier.
    Ident,
    /// An escaped identifier.
    EscapedIdent,
    /// A comment.
    Comment,
    /// A division operator.
    Div,
    /// A multiplication operator.
    Mul,
    /// A plus operator.
    Plus,
    /// A minus operator.
    Minus,
    /// A pipe character.
    Pipe,
    /// A double colon character.
    DoubleColon,
    /// A colon character.
    Colon,
    /// An integer.
    Integer,
    /// A float.
    Float,
    /// An equal comparison operator.
    EqualEqual,
    /// An equal sign.
    Equal,
    /// A variable reference.
    Variable,
    /// An escaped variable reference.
    EscapedVariable,
    /// A regex literal.
    Regex,
    /// A comma character.
    Comma,
    /// A open parenthesis `(`.
    ParenOpen,
    /// A close parenthesis `)`.
    ParenClose,
    /// A open bracket `[`.
    BracketOpen,
    /// A close bracket `]`.
    BracketClose,
    /// A open brace `{`.
    BraceOpen,
    /// A close brace `}`.
    BraceClose,
    /// A question mark `?`.
    QuestionMark,
    /// A bang `!`.
    Bang,
    /// A semicolon `;`.
    SemiColon,
    /// A less than or equal comparison operator.
    LessThanEqual,
    /// A greater than or equal comparison operator.
    GreaterThanEqual,
    /// A less than comparison operator.
    LessThan,
    /// A greater than comparison operator.
    GreaterThan,
    /// A not equal comparison operator.
    NotEqual,
    /// A dot dot `..` operator.
    DotDot,
    /// A string literal.
    String,
    /// A bool literal value.
    Bool,
    /// A inf literal value.
    Inf,
}

impl Token<'_> {
    /// Returns the start position of the token.
    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// Returns the length of the token.
    #[must_use]
    fn len(&self) -> usize {
        self.text().len()
    }

    /// Returns the end position of the token.
    #[must_use]
    pub fn end(&self) -> usize {
        self.pos() + self.len()
    }

    /// Returns the text  of the token.
    #[must_use]
    pub fn text(&self) -> &str {
        self.text
    }
    /// Returns the type of the token.
    #[must_use]
    pub fn tpe(&self) -> TokenType {
        self.tpe
    }
    /// returns if the token is invalid
    #[must_use]
    pub fn is_invalid(&self) -> bool {
        self.tpe == TokenType::Invalid
    }
    /// returns if the token is valid
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.tpe != TokenType::Invalid
    }
}

enum State {
    BraceOpen,
    StrOpen,
}

/// The lexer for the MPL query language.
pub struct Lexer<'input> {
    /// The input string to lex.
    input: &'input str,
    /// Peakable iterator of characters for UTF-8 compatibility
    chars: Peekable<Chars<'input>>,
    /// the current **byte** position in the input. for substring extraction.
    pos: usize,
    state: Vec<State>,
}

impl<'input> Lexer<'input> {
    /// Creates a new lexer for the given input.
    #[must_use]
    pub fn new(input: &'input str) -> Self {
        Self {
            input,
            chars: input.chars().peekable(),
            pos: 0,
            state: Vec::new(),
        }
    }

    /// Advances the lexer to the next character (could be multiple bytes for utf8)
    fn advance_char(&mut self) {
        let Some(c) = self.chars.next() else {
            return;
        };
        self.pos += c.len_utf8();
    }

    /// Parses a number int or float
    fn parse_number(&mut self, start: usize) -> Token<'input> {
        while self.chars.peek().is_some_and(char::is_ascii_digit) {
            self.advance_char();
        }
        if self.chars.peek().is_some_and(|c| *c == '.') {
            // We need a check here to be able to differentiate `0.` (float) from `0..` (range)
            // we know self.pos is a `.` so we can safely do +1 to check if the next character
            // is a `.` if it's a unicode codepoint that won't be true
            if self
                .input
                .as_bytes()
                .get(self.pos + 1)
                .is_some_and(|c| *c == b'.')
            {
                return Token {
                    tpe: TokenType::Integer,
                    text: &self.input[start..self.pos],
                    pos: start,
                };
            }
            self.advance_char();
            while self.chars.peek().is_some_and(char::is_ascii_digit) {
                self.advance_char();
            }
            Token {
                tpe: TokenType::Float,
                text: &self.input[start..self.pos],
                pos: start,
            }
        } else {
            Token {
                tpe: TokenType::Integer,
                text: &self.input[start..self.pos],
                pos: start,
            }
        }
    }

    /// Parses an escaped identifier.
    fn parse_escaped_ident(&mut self, start: usize) -> Token<'input> {
        while let Some(c) = self.chars.peek() {
            match c {
                '`' => break,
                '\\' => {
                    self.advance_char();
                    let Some(c) = self.chars.peek() else {
                        return Token {
                            tpe: TokenType::Invalid,
                            text: &self.input[start..self.pos],
                            pos: start,
                        };
                    };
                    if !(*c == '`' || *c == 'n' || *c == 't' || *c == 'r' || *c == '\\') {
                        return Token {
                            tpe: TokenType::Invalid,
                            text: &self.input[start..self.pos],
                            pos: start,
                        };
                    }
                }
                _ => {}
            }
            self.advance_char();
        }
        if self.chars.next() == Some('`') {
            self.pos += 1;
            Token {
                tpe: TokenType::EscapedIdent,
                text: &self.input[start..self.pos],
                pos: start,
            }
        } else {
            Token {
                tpe: TokenType::Invalid,
                text: &self.input[start..self.pos],
                pos: start,
            }
        }
    }

    fn parse_string(&mut self, start: usize) -> Token<'input> {
        self.state.push(State::StrOpen);
        while let Some(c) = self.chars.peek() {
            match c {
                '"' => {
                    break;
                }
                '\\' => {
                    self.advance_char();
                    // escape at the end of the input is invalid
                    if let Some(c) = self.chars.peek()
                        && matches!(c, '\\' | '"' | 'n' | 't' | 'r' | 'b' | 'f' | '$')
                    {
                        self.advance_char();
                    } else {
                        return Token {
                            tpe: TokenType::Invalid,
                            text: &self.input[start..self.pos],
                            pos: start,
                        };
                    }
                }
                '$' => {
                    self.advance_char();
                    if let Some(c) = self.chars.peek() {
                        // we only break if `$` is followed by `{`
                        // otherwise, $ is a normal character in a string literal
                        if *c == '{' {
                            break;
                        }
                    } else {
                        // there is no char after the dollar sign, so it's invalid
                        return Token {
                            tpe: TokenType::Invalid,
                            text: &self.input[start..self.pos],
                            pos: start,
                        };
                    }
                }
                _ => {
                    self.advance_char();
                }
            }
        }
        match self.chars.next() {
            Some('"') => {
                self.state.pop();
                // Oh no!  is this  a invalid string? Or should the tokenizer not care?
                // if !matches!(self.state.pop(), Some(State::StrOpen)) {
                //     return Token::Invalid(start, &self.input[start..self.pos]);
                // }
                self.pos += 1;
                Token {
                    tpe: TokenType::String,
                    text: &self.input[start..self.pos],
                    pos: start,
                }
            }
            Some('{') => {
                // we don't pop since we enter nested terretorry
                self.pos += 1;
                Token {
                    tpe: TokenType::String,
                    text: &self.input[start..self.pos],
                    pos: start,
                }
            }
            _ => Token {
                tpe: TokenType::Invalid,
                text: &self.input[start..self.pos],
                pos: start,
            },
        }
    }

    /// Parses a regex literal.
    fn parse_regex(&mut self, start: usize) -> Token<'input> {
        if self.chars.peek().is_some_and(|c| *c == '/') {
            self.advance_char();
            while let Some(c) = self.chars.peek() {
                match c {
                    '/' => break,
                    '\\' => {
                        self.advance_char();
                        let Some(c) = self.chars.peek() else {
                            return Token {
                                tpe: TokenType::Invalid,
                                text: &self.input[start..self.pos],
                                pos: start,
                            };
                        };
                        if !(*c == '/'
                            || *c == '\\'
                            || *c == '{'
                            || *c == '}'
                            || *c == '['
                            || *c == ']'
                            || *c == '('
                            || *c == ')'
                            || *c == '*'
                            || *c == '.'
                            || *c == '+'
                            || *c == '|'
                            || *c == '$'
                            || *c == 'n'
                            || *c == 't'
                            || *c == 'r')
                        {
                            return Token {
                                tpe: TokenType::Invalid,
                                text: &self.input[start..self.pos],
                                pos: start,
                            };
                        }
                    }
                    _ => {}
                }
                self.advance_char();
            }
            if self.chars.next() == Some('/') {
                self.pos += 1;
                Token {
                    tpe: TokenType::Regex,
                    text: &self.input[start..self.pos],
                    pos: start,
                }
            } else {
                Token {
                    tpe: TokenType::Invalid,
                    text: &self.input[start..self.pos],
                    pos: start,
                }
            }
        } else {
            Token {
                tpe: TokenType::Invalid,
                text: &self.input[start..self.pos],
                pos: start,
            }
        }
    }

    /// Parses an identifier or keyword.
    fn parse_ident_or_kw(&mut self, start: usize) -> Token<'input> {
        while self
            .chars
            .peek()
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            self.advance_char();
        }
        let ident = &self.input[start..self.pos];
        // check if we have a ident or a keyword
        match ident {
            "true" | "false" => Token {
                tpe: TokenType::Bool,
                text: ident,
                pos: start,
            },
            "inf" => Token {
                tpe: TokenType::Inf,
                text: ident,
                pos: start,
            },
            other => Token {
                tpe: TokenType::Ident,
                text: other,
                pos: start,
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    fn next_token(&mut self) -> Option<Token<'input>> {
        let pos = self.pos;
        let c = self.chars.next()?;
        self.pos += c.len_utf8();
        let token = match c {
            c if c.is_whitespace() => {
                while self.chars.peek().is_some_and(|c| c.is_whitespace()) {
                    self.advance_char();
                }
                Token {
                    tpe: TokenType::Whitespace,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            '|' => Token {
                tpe: TokenType::Pipe,
                text: &self.input[pos..self.pos],
                pos,
            },
            ',' => Token {
                tpe: TokenType::Comma,
                text: &self.input[pos..self.pos],
                pos,
            },
            '(' => Token {
                tpe: TokenType::ParenOpen,
                text: &self.input[pos..self.pos],
                pos,
            },
            ')' => Token {
                tpe: TokenType::ParenClose,
                text: &self.input[pos..self.pos],
                pos,
            },
            '[' => Token {
                tpe: TokenType::BracketOpen,
                text: &self.input[pos..self.pos],
                pos,
            },
            ']' => Token {
                tpe: TokenType::BracketClose,
                text: &self.input[pos..self.pos],
                pos,
            },
            '{' => {
                self.state.push(State::BraceOpen);
                Token {
                    tpe: TokenType::BraceOpen,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            '}' => match self.state.pop() {
                Some(State::BraceOpen) | None => Token {
                    tpe: TokenType::BraceClose,
                    text: &self.input[pos..self.pos],
                    pos,
                },
                Some(State::StrOpen) => self.parse_string(pos),
            },
            '?' => Token {
                tpe: TokenType::QuestionMark,
                text: &self.input[pos..self.pos],
                pos,
            },
            ';' => Token {
                tpe: TokenType::SemiColon,
                text: &self.input[pos..self.pos],
                pos,
            },
            '*' => Token {
                tpe: TokenType::Mul,
                text: &self.input[pos..self.pos],
                pos,
            },
            '+' => Token {
                tpe: TokenType::Plus,
                text: &self.input[pos..self.pos],
                pos,
            },
            '-' => Token {
                tpe: TokenType::Minus,
                text: &self.input[pos..self.pos],
                pos,
            },
            '.' if self.chars.peek().is_some_and(|c| *c == '.') => {
                self.advance_char();
                Token {
                    tpe: TokenType::DotDot,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            '!' if self.chars.peek().is_some_and(|c| *c == '=') => {
                self.advance_char();
                Token {
                    tpe: TokenType::NotEqual,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            '!' => Token {
                tpe: TokenType::Bang,
                text: &self.input[pos..self.pos],
                pos,
            },
            ':' if self.chars.peek().is_some_and(|c| *c == ':') => {
                self.advance_char();
                Token {
                    tpe: TokenType::DoubleColon,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            ':' => Token {
                tpe: TokenType::Colon,
                text: &self.input[pos..self.pos],
                pos,
            },
            '=' if self.chars.peek().is_some_and(|c| *c == '=') => {
                self.advance_char();
                Token {
                    tpe: TokenType::EqualEqual,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            '=' => Token {
                tpe: TokenType::Equal,
                text: &self.input[pos..self.pos],
                pos,
            },
            '<' if self.chars.peek().is_some_and(|c| *c == '=') => {
                self.advance_char();
                Token {
                    tpe: TokenType::LessThanEqual,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            '<' => Token {
                tpe: TokenType::LessThan,
                text: &self.input[pos..self.pos],
                pos,
            },
            '>' if self.chars.peek().is_some_and(|c| *c == '=') => {
                self.advance_char();
                Token {
                    tpe: TokenType::GreaterThanEqual,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            '>' => Token {
                tpe: TokenType::GreaterThan,
                text: &self.input[pos..self.pos],
                pos,
            },
            '/' if self.chars.peek().is_some_and(|c| *c == '/') => {
                while self.chars.peek().is_some_and(|c| *c != '\n') {
                    self.advance_char();
                }
                Token {
                    tpe: TokenType::Comment,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            '/' => Token {
                tpe: TokenType::Div,
                text: &self.input[pos..self.pos],
                pos,
            },
            c if c.is_alphabetic() || c == '_' => self.parse_ident_or_kw(pos),
            '$' if self.chars.peek().is_some_and(|c| *c == '`') => {
                self.advance_char();
                match self.parse_escaped_ident(pos) {
                    Token {
                        tpe: TokenType::EscapedIdent,
                        text,
                        pos,
                    } => Token {
                        tpe: TokenType::EscapedVariable,
                        text,
                        pos,
                    },
                    o => o,
                }
            }
            '$' if self
                .chars
                .peek()
                .is_some_and(|c| c.is_alphabetic() || *c == '_') =>
            {
                self.advance_char();
                while self
                    .chars
                    .peek()
                    .is_some_and(|c| c.is_alphanumeric() || *c == '_')
                {
                    self.advance_char();
                }
                Token {
                    tpe: TokenType::Variable,
                    text: &self.input[pos..self.pos],
                    pos,
                }
            }
            c if c.is_ascii_digit() => self.parse_number(pos),
            '`' => self.parse_escaped_ident(pos),
            '"' => self.parse_string(pos),
            '#' => self.parse_regex(pos),
            _ => Token {
                tpe: TokenType::Invalid,
                text: &self.input[pos..self.pos],
                pos,
            },
        };
        Some(token)
    }
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Token<'input>;

    #[allow(clippy::too_many_lines)]
    fn next(&mut self) -> Option<Token<'input>> {
        self.next_token()
    }
}
