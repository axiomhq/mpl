// Choices:
//
// - we do not allow unicode escapes
// - we do not allow rfc 3339 timestamps
//
use std::{iter::Peekable, str::Chars};

/// Represents a token parsed from the input.
pub enum Token<'input> {
    /// An invalid token.
    Invalid(usize, &'input str),
    /// Whitespace.
    Whitespace(usize, &'input str),
    /// An identifier.
    Ident(usize, &'input str),
    /// An escaped identifier.
    EscapedIdent(usize, &'input str),
    /// A comment.
    Comment(usize, &'input str),
    /// A division operator.
    Div(usize),
    /// A multiplication operator.
    Mul(usize),
    /// A plus operator.
    Plus(usize),
    /// A minus operator.
    Minus(usize),
    /// A pipe character.
    Pipe(usize),
    /// A double colon character.
    DoubleColon(usize),
    /// A colon character.
    Colon(usize),
    /// An integer.
    Integer(usize, &'input str),
    /// A float.
    Float(usize, &'input str),
    /// An equal comparison operator.
    EqualEqual(usize),
    /// An equal sign.
    Equal(usize),
    /// A variable reference.
    Variable(usize, &'input str),
    /// An escaped variable reference.
    EscapedVariable(usize, &'input str),
    /// A regex literal.
    Regex(usize, &'input str),
    /// A comma character.
    Comma(usize),
    /// A open parenthesis `(`.
    ParenOpen(usize),
    /// A close parenthesis `)`.
    ParenClose(usize),
    /// A open bracket `[`.
    BracketOpen(usize),
    /// A close bracket `]`.
    BracketClose(usize),
    /// A open brace `{`.
    BraceOpen(usize),
    /// A close brace `}`.
    BraceClose(usize),
    /// A question mark `?`.
    QuestionMark(usize),
    /// A bang `!`.
    Bang(usize),
    /// A semicolon `;`.
    SemiColon(usize),
    /// A less than or equal comparison operator.
    LessThanEqual(usize),
    /// A greater than or equal comparison operator.
    GreaterThanEqual(usize),
    /// A less than comparison operator.
    LessThan(usize),
    /// A greater than comparison operator.
    GreaterThan(usize),
    /// A not equal comparison operator.
    NotEqual(usize),
    /// A dot dot `..` operator.
    DotDot(usize),
    /// A string literal.
    String(usize, &'input str),
    /// A bool literal value.
    Bool(usize, &'input str),
    /// A inf literal value.
    Inf(usize, &'input str),
}

enum State {
    BraceOpen,
    StrOpen,
}

/// The lexer for the MPL query language.
pub struct Lexer<'input> {
    /// The input string to lex.
    input: &'input str,
    /// Peakable itterator ofer characters for utf8 compatibility
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
                return Token::Integer(start, &self.input[start..self.pos]);
            }
            self.advance_char();
            while self.chars.peek().is_some_and(char::is_ascii_digit) {
                self.advance_char();
            }
            Token::Float(start, &self.input[start..self.pos])
        } else {
            Token::Integer(start, &self.input[start..self.pos])
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
                        return Token::Invalid(start, &self.input[start..self.pos]);
                    };
                    if !(*c == '`' || *c == 'n' || *c == 't' || *c == 'r' || *c == '\\') {
                        return Token::Invalid(start, &self.input[start..self.pos]);
                    }
                }
                _ => {}
            }
            self.advance_char();
        }
        if self.chars.next() == Some('`') {
            self.pos += 1;
            Token::EscapedIdent(start, &self.input[start..self.pos])
        } else {
            Token::Invalid(start, &self.input[start..self.pos])
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
                        return Token::Invalid(start, &self.input[start..self.pos]);
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
                        return Token::Invalid(start, &self.input[start..self.pos]);
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
                Token::String(start, &self.input[start..self.pos])
            }
            Some('{') => {
                // we don't pop since we enter nested terretorry
                self.pos += 1;
                Token::String(start, &self.input[start..self.pos])
            }
            _ => Token::Invalid(start, &self.input[start..self.pos]),
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
                            return Token::Invalid(start, &self.input[start..self.pos]);
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
                            return Token::Invalid(start, &self.input[start..self.pos]);
                        }
                    }
                    _ => {}
                }
                self.advance_char();
            }
            if self.chars.next() == Some('/') {
                self.pos += 1;
                Token::Regex(start, &self.input[start..self.pos])
            } else {
                Token::Invalid(start, &self.input[start..self.pos])
            }
        } else {
            Token::Invalid(start, &self.input[start..self.pos])
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
            "true" | "false" => Token::Bool(start, ident),
            "inf" => Token::Inf(start, ident),
            other => Token::Ident(start, other),
        }
    }

    fn next_token(&mut self) -> Option<Token<'input>> {
        let start = self.pos;
        let c = self.chars.next()?;
        self.pos += c.len_utf8();
        let token = match c {
            c if c.is_whitespace() => {
                while self.chars.peek().is_some_and(|c| c.is_whitespace()) {
                    self.advance_char();
                }
                Token::Whitespace(start, &self.input[start..self.pos])
            }
            '|' => Token::Pipe(start),
            ',' => Token::Comma(start),
            '(' => Token::ParenOpen(start),
            ')' => Token::ParenClose(start),
            '[' => Token::BracketOpen(start),
            ']' => Token::BracketClose(start),
            '{' => {
                self.state.push(State::BraceOpen);
                Token::BraceOpen(start)
            }
            '}' => match self.state.pop() {
                Some(State::BraceOpen) | None => Token::BraceClose(start),
                Some(State::StrOpen) => self.parse_string(start),
            },
            '?' => Token::QuestionMark(start),
            ';' => Token::SemiColon(start),
            '*' => Token::Mul(start),
            '+' => Token::Plus(start),
            '-' => Token::Minus(start),
            '.' if self.chars.peek().is_some_and(|c| *c == '.') => {
                self.advance_char();
                Token::DotDot(start)
            }
            '!' if self.chars.peek().is_some_and(|c| *c == '=') => {
                self.advance_char();
                Token::NotEqual(start)
            }
            '!' => Token::Bang(start),
            ':' if self.chars.peek().is_some_and(|c| *c == ':') => {
                self.advance_char();
                Token::DoubleColon(start)
            }
            ':' => Token::Colon(start),
            '=' if self.chars.peek().is_some_and(|c| *c == '=') => {
                self.advance_char();
                Token::EqualEqual(start)
            }
            '=' => Token::Equal(start),
            '<' if self.chars.peek().is_some_and(|c| *c == '=') => {
                self.advance_char();
                Token::LessThanEqual(start)
            }
            '<' => Token::LessThan(start),
            '>' if self.chars.peek().is_some_and(|c| *c == '=') => {
                self.advance_char();
                Token::GreaterThanEqual(start)
            }
            '>' => Token::GreaterThan(start),
            '/' if self.chars.peek().is_some_and(|c| *c == '/') => {
                while self.chars.peek().is_some_and(|c| *c != '\n') {
                    self.advance_char();
                }
                Token::Comment(start, &self.input[start..self.pos])
            }
            '/' => Token::Div(start),
            c if c.is_alphabetic() || c == '_' => self.parse_ident_or_kw(start),
            '$' if self.chars.peek().is_some_and(|c| *c == '`') => {
                self.advance_char();
                match self.parse_escaped_ident(start) {
                    Token::EscapedIdent(s, token) => Token::EscapedVariable(s, token),
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
                Token::Variable(start, &self.input[start..self.pos])
            }
            c if c.is_ascii_digit() => self.parse_number(start),
            '`' => self.parse_escaped_ident(start),
            '"' => self.parse_string(start),
            '#' => self.parse_regex(start),
            _ => Token::Invalid(start, &self.input[start..self.pos]),
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
