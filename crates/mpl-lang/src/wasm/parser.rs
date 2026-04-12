#![allow(dead_code)]
use crate::wasm::Span;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Type {
    Ident,
    ParenOpen,
    ParneClose,
    Comma,
    Whitespace,
    Pipe,
    DoubleColon,
    Colon,
    Equals,
    Not,
    NotEquals,
    SquareBracketClose,
    SquareBracketOpen,
    DoubleDot,
    Dot,
    GreaterThanOrEqual,
    GreaterThan,
    LessThanOrEqual,
    LessThan,
    Float,
    Integer,
    Boolean,
    Duration,
    Operator,
    Semicolon,
    Slash,
    Star,
    Comment,
    Regex,
}
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Token<'src> {
    typ: Type,
    value: &'src str,
    span: Span,
}

fn parse_integer(src: &str) -> usize {
    let mut start = 0;
    while let Some(b) = src.as_bytes().get(start)
        && b.is_ascii_digit()
    {
        start += 1;
    }
    start
}
fn parse_float_fraction(src: &str) -> usize {
    let mut start = parse_integer(src);
    if let Some(b) = src.as_bytes().get(start)
        && b == &b'e'
    {
        start += 1 + parse_integer(&src[start + 1..]);
    }
    start
}

fn is_kw(s: &str) -> bool {
    matches!(
        s,
        "not"
            | "filter"
            | "where"
            | "sample"
            | "and"
            | "or"
            | "map"
            | "align"
            | "to"
            | "over"
            | "using"
            | "bucket"
            | "group"
            | "by"
            | "compute"
            | "set"
    )
}

#[allow(clippy::too_many_lines)]
pub(crate) fn tokenize(src: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut i = 0;
    loop {
        let Some(c) = src.as_bytes().get(i) else {
            return tokens;
        };
        match *c {
            b';' => {
                tokens.push(Token {
                    typ: Type::Semicolon,
                    value: ";",
                    span: Span::new(i, i + 1),
                });
                i += 1;
            }
            b'*' => {
                tokens.push(Token {
                    typ: Type::Star,
                    value: "(",
                    span: Span::new(i, i + 1),
                });
                i += 1;
            }
            b'#' => {
                let start = i;
                let next = src.as_bytes().get(i + 1).copied();
                // #/ starts a regex literal, #s/ starts a regex replace
                let is_regex = next == Some(b'/');
                let is_regex_replace = next == Some(b's')
                    && src.as_bytes().get(i + 2).copied() == Some(b'/');
                if is_regex || is_regex_replace {
                    // skip past the opening delimiter (#/ or #s/)
                    i += if is_regex_replace { 3 } else { 2 };
                    // count how many closing `/` we need (1 for regex, 2 for replace)
                    let slashes_needed: u8 = if is_regex_replace { 2 } else { 1 };
                    let mut slashes_seen: u8 = 0;
                    while i < src.len() && slashes_seen < slashes_needed {
                        if src.as_bytes()[i] == b'\\' && i + 1 < src.len() {
                            i += 2; // skip escape sequence
                        } else if src.as_bytes()[i] == b'/' {
                            slashes_seen += 1;
                            i += 1;
                        } else {
                            i += 1;
                        }
                    }
                    tokens.push(Token {
                        typ: Type::Regex,
                        value: &src[start..i],
                        span: Span::new(start, i),
                    });
                } else {
                    // bare `#` is not valid syntax; skip it
                    i += 1;
                }
            }
            b'/' => {
                if src.as_bytes().get(i + 1) == Some(&b'/') {
                    let start = i;
                    i += 2;
                    while src.as_bytes().get(i) != Some(&b'\n') && i < src.len() {
                        i += 1;
                    }
                    tokens.push(Token {
                        typ: Type::Comment,
                        value: &src[start..i],
                        span: Span::new(start, i),
                    });
                } else {
                    tokens.push(Token {
                        typ: Type::Slash,
                        value: "/",
                        span: Span::new(i, i + 1),
                    });
                    i += 1;
                }
            }
            b'+' | b'-' => {
                tokens.push(Token {
                    typ: Type::Operator,
                    value: std::str::from_utf8(&[*c]).unwrap(),
                    span: Span::new(i, i + 1),
                });
                i += 1;
            }
            b'(' => {
                tokens.push(Token {
                    typ: Type::ParenOpen,
                    value: "(",
                    span: Span::new(i, i + 1),
                });
                i += 1;
            }
            b')' => {
                tokens.push(Token {
                    typ: Type::ParneClose,
                    value: ")",
                    span: Span::new(i, i + 1),
                });
                i += 1;
            }
            b'[' => {
                tokens.push(Token {
                    typ: Type::SquareBracketOpen,
                    value: "[",
                    span: Span::new(i, i + 1),
                });
                i += 1;
            }
            b']' => {
                tokens.push(Token {
                    typ: Type::SquareBracketClose,
                    value: "]",
                    span: Span::new(i, i + 1),
                });
                i += 1;
            }
            b'.' => {
                if src.as_bytes().get(i + 1) == Some(&b'.') {
                    tokens.push(Token {
                        typ: Type::DoubleDot,
                        value: "..",
                        span: Span::new(i, i + 2),
                    });
                    i += 2;
                } else if let Some(b) = src.as_bytes().get(i + 1)
                    && b.is_ascii_digit()
                {
                    let start = i;
                    i += 1 + parse_float_fraction(&src[i + 1..]);
                    tokens.push(Token {
                        typ: Type::Float,
                        value: &src[start..i],
                        span: Span::new(start, i),
                    });
                } else {
                    tokens.push(Token {
                        typ: Type::Dot,
                        value: ".",
                        span: Span::new(i, i + 1),
                    });
                    i += 1;
                }
            }
            b',' => {
                tokens.push(Token {
                    typ: Type::Comma,
                    value: ",",
                    span: Span::new(i, i + 1),
                });
                i += 1;
            }
            b'|' => {
                tokens.push(Token {
                    typ: Type::Pipe,
                    value: "|",
                    span: Span::new(i, i + 1),
                });
                i += 1;
            }
            b':' => {
                if src.as_bytes().get(i + 1) == Some(&b':') {
                    tokens.push(Token {
                        typ: Type::DoubleColon,
                        value: "::",
                        span: Span::new(i, i + 2),
                    });
                    i += 2;
                } else {
                    tokens.push(Token {
                        typ: Type::Colon,
                        value: ":",
                        span: Span::new(i, i + 1),
                    });
                    i += 1;
                }
            }
            b if char::from(b).is_whitespace() => {
                let start = i;
                i += 1;
                while let Some(b) = src.as_bytes().get(i)
                    && char::from(*b).is_whitespace()
                {
                    i += 1;
                }
                let value = &src[start..i];
                tokens.push(Token {
                    typ: Type::Ident,
                    value,
                    span: Span::new(start, i),
                });
            }
            b'a'..=b'z' | b'A'..=b'Z' => {
                let start = i;
                i += 1;
                while let Some(b) = src.as_bytes().get(i)
                    && (b.is_ascii_lowercase()
                        || b.is_ascii_uppercase()
                        || b.is_ascii_digit()
                        || *b == b'_')
                {
                    i += 1;
                }
                let value = &src[start..i];
                match value {
                    "true" | "false" => {
                        tokens.push(Token {
                            typ: Type::Boolean,
                            value,
                            span: Span::new(start, i),
                        });
                    }
                    _ => {
                        tokens.push(Token {
                            typ: Type::Ident,
                            value,
                            span: Span::new(start, i),
                        });
                    }
                }
                tokens.push(Token {
                    typ: Type::Ident,
                    value,
                    span: Span::new(start, i),
                });
            }
            b'=' => {
                let Some(b) = src.as_bytes().get(i + 1) else {
                    // FIXME error
                    return tokens;
                };
                if *b != b'=' {
                    return tokens;
                }
                tokens.push(Token {
                    typ: Type::Equals,
                    value: "==",
                    span: Span::new(i, i + 2),
                });
                i += 2;
            }
            b'!' => {
                if let Some(b) = src.as_bytes().get(i + 1)
                    && *b == b'='
                {
                    tokens.push(Token {
                        typ: Type::NotEquals,
                        value: "!=",
                        span: Span::new(i, i + 2),
                    });
                    i += 2;
                } else {
                    tokens.push(Token {
                        typ: Type::Not,
                        value: "!",
                        span: Span::new(i, i + 1),
                    });
                    i += 1;
                }
            }
            b'<' => {
                if let Some(b) = src.as_bytes().get(i + 1)
                    && *b == b'='
                {
                    tokens.push(Token {
                        typ: Type::LessThanOrEqual,
                        value: "<=",
                        span: Span::new(i, i + 2),
                    });
                    i += 2;
                } else {
                    tokens.push(Token {
                        typ: Type::LessThan,
                        value: "<",
                        span: Span::new(i, i + 1),
                    });
                    i += 1;
                }
            }
            b'>' => {
                if let Some(b) = src.as_bytes().get(i + 1)
                    && *b == b'='
                {
                    tokens.push(Token {
                        typ: Type::GreaterThanOrEqual,
                        value: ">=",
                        span: Span::new(i, i + 2),
                    });
                    i += 2;
                } else {
                    tokens.push(Token {
                        typ: Type::GreaterThan,
                        value: ">",
                        span: Span::new(i, i + 1),
                    });
                    i += 1;
                }
            }
            b'0'..=b'9' => {
                let start = i;
                let n = parse_integer(&src[i..]);
                i += n;
                if let Some(b) = src.as_bytes().get(i)
                    && b == &b'.'
                {
                    i += 1;
                    let n = parse_float_fraction(&src[i..]);
                    i += n;
                    tokens.push(Token {
                        typ: Type::Float,
                        value: &src[start..i],
                        span: Span::new(start, i),
                    });
                } else if let Some(b) = src.as_bytes().get(i)
                    && b == &b'm'
                {
                    i += 1;
                    if let Some(b) = src.as_bytes().get(i)
                        && *b == b's'
                    {
                        i += 1;
                    }
                    tokens.push(Token {
                        typ: Type::Duration,
                        value: &src[start..i],
                        span: Span::new(start, i),
                    });
                } else if let Some(b) = src.as_bytes().get(i)
                    && (b == &b's'
                        || b == &b'h'
                        || b == &b'd'
                        || b == &b'w'
                        || b == &b'M'
                        || b == &b'y')
                {
                    i += 1;
                    tokens.push(Token {
                        typ: Type::Duration,
                        value: &src[start..i],
                        span: Span::new(start, i),
                    });
                } else {
                    tokens.push(Token {
                        typ: Type::Integer,
                        value: &src[start..i],
                        span: Span::new(start, i),
                    });
                }
            }

            _ => {
                // FIXME: add invalid character handling
                return tokens;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_ident() {
        let src = "foo_bar";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Ident,
                value: "foo_bar",
                span: Span::new(0, 7),
            }]
        );
    }
    #[test]
    fn test_integer() {
        let src = "42";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Integer,
                value: "42",
                span: Span::new(0, 2),
            }]
        );
    }
    #[test]
    fn test_float() {
        let src = "42.0";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Float,
                value: "42.0",
                span: Span::new(0, 4),
            }]
        );
    }
    #[test]
    fn test_float_exponent() {
        let src = "42.0e1";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Float,
                value: "42.0e1",
                span: Span::new(0, 6),
            }]
        );
    }
    #[test]
    fn test_float_no_integer() {
        let src = ".0";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Float,
                value: ".0",
                span: Span::new(0, 2),
            }]
        );
    }
    #[test]
    fn test_duration() {
        let src = "42s";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Duration,
                value: "42s",
                span: Span::new(0, 3),
            }]
        );
    }
    #[test]
    fn test_duratuion_ms() {
        let src = "42ms";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Duration,
                value: "42ms",
                span: Span::new(0, 4),
            }]
        );
    }
    #[test]
    fn test_regex() {
        let src = "#/foo/";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Regex,
                value: "#/foo/",
                span: Span::new(0, 6),
            }]
        );
    }
    #[test]
    fn test_regex_replace() {
        let src = "#s/src/dst/";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Regex,
                value: "#s/src/dst/",
                span: Span::new(0, 11),
            }]
        );
    }
    #[test]
    fn test_comment() {
        let src = "// this is a comment";
        let tokens = tokenize(src);
        assert_eq!(
            tokens,
            vec![Token {
                typ: Type::Comment,
                value: "// this is a comment",
                span: Span::new(0, 20),
            }]
        );
    }
}
