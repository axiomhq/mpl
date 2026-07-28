//! Token-stream tests for the MPL lexer.
//!
//! Cases are written as `source => expected token stream`, where the expected side is a
//! compact rendering: operators render as their own source text (`<=`, `..`), tokens that
//! carry text render as `Kind(text)`. Writing the whole stream on one line is what makes a
//! table readable; the alternative — building `Vec<Token>` literals — buries the signal in
//! offsets that are better checked by the structural properties at the bottom of this file.
//!
//! Offsets are deliberately absent from the tables. They are covered exhaustively by
//! `assert_tiles`, which is a stronger check than spot-asserting a few numbers by hand.

use mpl_lang::lexer::{Lexer, Token};
use test_case::test_case;

/// Decomposes a token into `(start, kind, source text)`.
///
/// Every other helper derives from this, so the 37-variant list is written exactly once and
/// a newly added variant fails to compile here rather than being silently skipped.
///
/// For operators the kind string *is* the source text the lexer consumed, which is what lets
/// `span_len` work without the tokens carrying a length. `assert_tiles` verifies that
/// coupling on every input, so a mismatch surfaces as a failing test rather than as silent
/// drift.
fn parts<'input>(token: &Token<'input>) -> (usize, &'static str, Option<&'input str>) {
    match *token {
        Token::Invalid(p, s) => (p, "Invalid", Some(s)),
        Token::Whitespace(p, s) => (p, "WS", Some(s)),
        Token::Ident(p, s) => (p, "Ident", Some(s)),
        Token::Keyword(p, s) => (p, "Kw", Some(s)),
        Token::EscapedIdent(p, s) => (p, "EscIdent", Some(s)),
        Token::Comment(p, s) => (p, "Comment", Some(s)),
        Token::Integer(p, s) => (p, "Int", Some(s)),
        Token::Float(p, s) => (p, "Float", Some(s)),
        Token::Variable(p, s) => (p, "Var", Some(s)),
        Token::EscapedVariable(p, s) => (p, "EscVar", Some(s)),
        Token::Regex(p, s) => (p, "Regex", Some(s)),
        Token::String(p, s) => (p, "Str", Some(s)),
        Token::Div(p) => (p, "/", None),
        Token::Mul(p) => (p, "*", None),
        Token::Plus(p) => (p, "+", None),
        Token::Minus(p) => (p, "-", None),
        Token::Pipe(p) => (p, "|", None),
        Token::DoubleColon(p) => (p, "::", None),
        Token::Colon(p) => (p, ":", None),
        Token::EqualEqual(p) => (p, "==", None),
        Token::Equal(p) => (p, "=", None),
        Token::Comma(p) => (p, ",", None),
        Token::ParenOpen(p) => (p, "(", None),
        Token::ParenClose(p) => (p, ")", None),
        Token::BracketOpen(p) => (p, "[", None),
        Token::BracketClose(p) => (p, "]", None),
        Token::BraceOpen(p) => (p, "{", None),
        Token::BraceClose(p) => (p, "}", None),
        Token::QuestionMark(p) => (p, "?", None),
        Token::Bang(p) => (p, "!", None),
        Token::SemiColon(p) => (p, ";", None),
        Token::LessThanEqual(p) => (p, "<=", None),
        Token::GreaterThanEqual(p) => (p, ">=", None),
        Token::LessThan(p) => (p, "<", None),
        Token::GreaterThan(p) => (p, ">", None),
        Token::NotEqual(p) => (p, "!=", None),
        Token::DotDot(p) => (p, "..", None),
    }
}

/// Renders one token for the expected column. Whitespace collapses to `WS` because its
/// actual text is noise in a table; its real extent is still checked by `assert_tiles`.
fn describe(token: &Token<'_>) -> String {
    let (_, kind, text) = parts(token);
    if matches!(token, Token::Whitespace(..)) {
        return "WS".to_string();
    }
    text.map_or_else(|| kind.to_string(), |s| format!("{kind}({s})"))
}

/// How many bytes of input this token consumed.
fn span_len(token: &Token<'_>) -> usize {
    let (_, kind, text) = parts(token);
    text.map_or(kind.len(), str::len)
}

/// Lexes into the compact form, dropping whitespace — the default for the tables below.
fn lex(input: &str) -> String {
    Lexer::new(input)
        .filter(|t| !matches!(t, Token::Whitespace(..)))
        .map(|t| describe(&t))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lexes into the compact form, keeping whitespace tokens.
fn lex_ws(input: &str) -> String {
    Lexer::new(input)
        .map(|t| describe(&t))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------------------
// Operators
//
// The `<>` / `=<` / `=>` cases are regression guards: each was accepted as a single
// comparison token at some point, and none of them is an MPL operator (mpl.pest:84).
// ---------------------------------------------------------------------------------------

#[test_case("a == 1" => "Ident(a) == Int(1)"     ; "equal equal")]
#[test_case("a != 1" => "Ident(a) != Int(1)"     ; "not equal")]
#[test_case("a = 1"  => "Ident(a) = Int(1)"      ; "assign")]
#[test_case("a < 1"  => "Ident(a) < Int(1)"      ; "less than")]
#[test_case("a <= 1" => "Ident(a) <= Int(1)"     ; "less than equal")]
#[test_case("a > 1"  => "Ident(a) > Int(1)"      ; "greater than")]
#[test_case("a >= 1" => "Ident(a) >= Int(1)"     ; "greater than equal")]
#[test_case("a <> 1" => "Ident(a) < > Int(1)"    ; "diamond is not an operator")]
#[test_case("a =< 1" => "Ident(a) = < Int(1)"    ; "reversed le is not an operator")]
#[test_case("a => 1" => "Ident(a) = > Int(1)"    ; "reversed ge is not an operator")]
#[test_case("a ! b"  => "Ident(a) ! Ident(b)"    ; "bare bang")]
#[test_case("+ - * /" => "+ - * /"               ; "arithmetic")]
#[test_case("a::b"   => "Ident(a) :: Ident(b)"   ; "double colon")]
#[test_case("a:b"    => "Ident(a) : Ident(b)"    ; "single colon")]
#[test_case("(){}[],;?" => "( ) { } [ ] , ; ?"   ; "punctuation")]
fn operators(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Numbers and ranges
//
// `..` and a decimal point compete for the same character. Both directions matter: a range
// must not eat a float's dot, and a float must not eat a range's first dot.
// ---------------------------------------------------------------------------------------

#[test_case("0"        => "Int(0)"                 ; "zero")]
#[test_case("42"       => "Int(42)"                ; "integer")]
#[test_case("1.5"      => "Float(1.5)"             ; "float")]
#[test_case("2."       => "Float(2.)"              ; "trailing dot float is legal per mpl.pest:31")]
#[test_case("f(2., 3)" => "Ident(f) ( Float(2.) , Int(3) )" ; "trailing dot before delimiter")]
#[test_case("300..600" => "Int(300) .. Int(600)"   ; "timestamp range")]
#[test_case("1..2"     => "Int(1) .. Int(2)"       ; "single digit range")]
#[test_case("[2.5..3]" => "[ Float(2.5) .. Int(3) ]" ; "float then range")]
#[test_case("[1..2.5]" => "[ Int(1) .. Float(2.5) ]" ; "range then float")]
#[test_case("[5m..]"   => "[ Int(5) Ident(m) .. ]" ; "relative open ended range")]
#[test_case("[..600]"  => "[ .. Int(600) ]"        ; "open start range")]
#[test_case("1700000000..1700003600" => "Int(1700000000) .. Int(1700003600)" ; "unix timestamp range")]
#[test_case("1...2"    => "Int(1) .. Invalid(.) Int(2)" ; "three dots is not valid syntax")]
#[test_case("a . b"    => "Ident(a) Invalid(.) Ident(b)" ; "lone dot is invalid")]
fn numbers_and_ranges(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Identifiers and keywords
// ---------------------------------------------------------------------------------------

#[test_case("foo"       => "Ident(foo)"          ; "plain")]
#[test_case("_foo"      => "Ident(_foo)"         ; "leading underscore")]
#[test_case("foo_bar1"  => "Ident(foo_bar1)"     ; "digits and underscores")]
#[test_case("where"     => "Kw(where)"           ; "keyword")]
#[test_case("group by"  => "Kw(group) Kw(by)"    ; "two keywords")]
#[test_case("wherever"  => "Ident(wherever)"     ; "keyword prefix is still an ident")]
fn identifiers(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Escaped identifiers
//
// The escape arm must consume exactly the backslash and the escaped character. Consuming
// one char too many silently swallowed the closing delimiter, which is why every case here
// puts an escape immediately before the terminator.
// ---------------------------------------------------------------------------------------

#[test_case("`foo`"    => "EscIdent(`foo`)"      ; "plain")]
#[test_case("``"       => "EscIdent(``)"         ; "empty")]
#[test_case("`a b`"    => "EscIdent(`a b`)"      ; "with space")]
#[test_case(r"`a\n`"   => r"EscIdent(`a\n`)"     ; "escape before terminator")]
#[test_case(r"`a\nb`"  => r"EscIdent(`a\nb`)"    ; "escape mid literal")]
#[test_case(r"`a\``"   => r"EscIdent(`a\``)"     ; "escaped backtick before terminator")]
#[test_case(r"`a\\`"   => r"EscIdent(`a\\`)"     ; "escaped backslash before terminator")]
#[test_case(r"`a\t\r`" => r"EscIdent(`a\t\r`)"   ; "consecutive escapes")]
#[test_case("`abc"     => "Invalid(`abc)"        ; "unterminated")]
fn escaped_identifiers(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Strings
//
// A string literal and an escaped identifier are different productions (mpl.pest:19 vs :5),
// so they must not collapse to the same token kind.
// ---------------------------------------------------------------------------------------

#[test_case("\"foo\""       => "Str(\"foo\")"                  ; "plain")]
#[test_case("\"\""          => "Str(\"\")"                     ; "empty")]
#[test_case("\"foo\" `foo`" => "Str(\"foo\") EscIdent(`foo`)"  ; "string is distinct from escaped ident")]
#[test_case("\"abc"         => "Invalid(\"abc)"                ; "unterminated")]
fn strings(src: &str) -> String {
    lex(src)
}

// One case per member of `parse_string`'s escape whitelist, so narrowing it cannot pass
// unnoticed. Each escape sits immediately before the closing quote: an escape arm that
// consumes one character too many swallows the terminator, which is the failure mode the
// escaped-identifier tests above were written for.
#[test_case(r#""say \"hi\"""# => r#"Str("say \"hi\"")"# ; "escaped quote")]
#[test_case(r#""a\\""#        => r#"Str("a\\")"#        ; "escaped backslash")]
#[test_case(r#""a\n\t\r""#    => r#"Str("a\n\t\r")"#    ; "escaped control characters")]
#[test_case(r#""a\b\f""#      => r#"Str("a\b\f")"#      ; "escaped backspace and form feed")]
#[test_case(r#""a\q""#        => r#"Invalid("a\) Ident(q) Invalid(")"# ; "escape outside the whitelist is rejected")]
#[test_case(r#""a\"#          => r#"Invalid("a\)"#      ; "backslash at end of input")]
fn string_escapes(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// String interpolation
//
// `${` does not get a token of its own: it terminates the `Str` fragment that precedes it
// and is included in that fragment's text, and the matching `}` opens the fragment that
// follows it. So `"a${x}b"` is three tokens — `Str("a${)`, `Ident(x)`, `Str(}b")` — and a
// consumer reassembles the literal by stripping the `${` / `}` markers from the fragments.
// The tests are written against that shape deliberately: the invariant worth pinning is
// that the interpolation markers stay attached to the string fragments and that the body
// between them is lexed as ordinary MPL, not as string content.
//
// The competing reading of `$` is a variable (src/lexer.rs:355-376), so every case where a
// `$` is *not* followed by `{` must stay inside the literal.
// ---------------------------------------------------------------------------------------

#[test_case(r#""${x}""#        => r#"Str("${) Ident(x) Str(}")"#            ; "whole string is one interpolation")]
#[test_case(r#""a${x}b""#      => r#"Str("a${) Ident(x) Str(}b")"#          ; "text on both sides")]
#[test_case(r#""${x}b""#       => r#"Str("${) Ident(x) Str(}b")"#           ; "text after only")]
#[test_case(r#""a${x}""#       => r#"Str("a${) Ident(x) Str(}")"#           ; "text before only")]
#[test_case(r#""${}""#         => r#"Str("${) Str(}")"#                     ; "empty interpolation")]
#[test_case(r#""a${x}b${y}c""# => r#"Str("a${) Ident(x) Str(}b${) Ident(y) Str(}c")"# ; "two interpolations")]
#[test_case(r#""${x}${y}""#    => r#"Str("${) Ident(x) Str(}${) Ident(y) Str(}")"#    ; "adjacent interpolations")]
// The body is a token stream, not a substring: operators, keywords, numbers, variables and
// regexes all lex as themselves inside `${ }`.
#[test_case(r#""${a + b}""#    => r#"Str("${) Ident(a) + Ident(b) Str(}")"# ; "expression body")]
#[test_case(r#""${a:b}""#      => r#"Str("${) Ident(a) : Ident(b) Str(}")"# ; "colon in body")]
#[test_case(r#""${$foo}""#     => r#"Str("${) Var($foo) Str(}")"#           ; "variable body")]
#[test_case(r#""${where}""#    => r#"Str("${) Kw(where) Str(}")"#           ; "keyword body")]
#[test_case(r#""${1.5}""#      => r#"Str("${) Float(1.5) Str(}")"#          ; "float body")]
#[test_case(r#""${#/a/}""#     => r#"Str("${) Regex(#/a/) Str(}")"#         ; "regex body")]
#[test_case(r#""${f(a, b)}""#  => r#"Str("${) Ident(f) ( Ident(a) , Ident(b) ) Str(}")"# ; "call body")]
// A `$` only opens an interpolation when `{` follows it; everything else stays literal.
#[test_case(r#""a$b""#         => r#"Str("a$b")"#                           ; "dollar before an ident is literal")]
#[test_case(r#""a$""#          => r#"Str("a$")"#                            ; "dollar before the terminator is literal")]
#[test_case(r#""$""#           => r#"Str("$")"#                             ; "lone dollar is literal")]
#[test_case(r#""$$x""#         => r#"Str("$$x")"#                           ; "double dollar is literal")]
#[test_case(r#""a\${x}""#      => r#"Str("a\${x}")"#                        ; "escaped dollar suppresses interpolation")]
#[test_case(r#""héllo ${x}""#  => r#"Str("héllo ${) Ident(x) Str(}")"#      ; "multi byte text before an interpolation")]
#[test_case(r#""${é}""#        => r#"Str("${) Ident(é) Str(}")"#            ; "multi byte ident in the body")]
fn string_interpolation(src: &str) -> String {
    lex(src)
}

/// Whitespace inside `${ }` is a `Whitespace` token, not string content — the clearest
/// single demonstration that the body leaves string-literal mode entirely.
#[test_case(r#""${ x }""#   => r#"Str("${) WS Ident(x) WS Str(}")"#  ; "spaces around the body")]
#[test_case(r#""a b${ c }""# => r#"Str("a b${) WS Ident(c) WS Str(}")"# ; "spaces in the literal stay in the fragment")]
fn interpolation_whitespace(src: &str) -> String {
    lex_ws(src)
}

// ---------------------------------------------------------------------------------------
// Nested string interpolation
//
// Nesting is what the state stack in the lexer exists for: `{` pushes `BraceOpen`, an open
// string literal pushes `StrOpen`, and a `}` resumes a string only when `StrOpen` is on
// top. Every case below is a different way for the two to interleave, because getting the
// stack discipline wrong shows up as a `}` being closed against the wrong opener rather
// than as a malformed token.
// ---------------------------------------------------------------------------------------

#[test_case(
    r#""${"b"}""#
    => r#"Str("${) Str("b") Str(}")"#
    ; "string literal inside an interpolation"
)]
#[test_case(
    r#""a${"b"}c""#
    => r#"Str("a${) Str("b") Str(}c")"#
    ; "string literal inside an interpolation with surrounding text"
)]
#[test_case(
    r#""a${"b${c}d"}e""#
    => r#"Str("a${) Str("b${) Ident(c) Str(}d") Str(}e")"#
    ; "interpolation inside an interpolated string"
)]
#[test_case(
    r#""a${"b${"c${d}e"}f"}g""#
    => r#"Str("a${) Str("b${) Str("c${) Ident(d) Str(}e") Str(}f") Str(}g")"#
    ; "three levels deep"
)]
#[test_case(
    r#""${"a" + "b"}""#
    => r#"Str("${) Str("a") + Str("b") Str(}")"#
    ; "two sibling strings in one body"
)]
// A `}` inside a nested literal is string content, so it must not pop the outer `StrOpen`
// and end the interpolation early.
#[test_case(
    r#""${"}"}""#
    => r#"Str("${) Str("}") Str(}")"#
    ; "close brace inside a nested literal is content"
)]
#[test_case(
    r#""${"${"}""#
    => r#"Str("${) Str("${) Str("}")"#
    ; "dollar brace inside a nested literal opens another level"
)]
// Braces and interpolations interleaved: a `{` pushed inside a body must be popped by its
// own `}` before the interpolation's `}` is reached.
#[test_case(
    r#""${{a}}""#
    => r#"Str("${) { Ident(a) } Str(}")"#
    ; "brace group inside a body"
)]
#[test_case(
    r#""${ {a: "x"} }""#
    => r#"Str("${) { Ident(a) : Str("x") } Str(}")"#
    ; "brace group containing a string"
)]
#[test_case(
    r#"{ "a${b}c" }"#
    => r#"{ Str("a${) Ident(b) Str(}c") }"#
    ; "interpolated string inside a brace group"
)]
#[test_case(
    r#"{ "a${ {b} }c" }"#
    => r#"{ Str("a${) { Ident(b) } Str(}c") }"#
    ; "brace group inside an interpolation inside a brace group"
)]
// Once a literal is closed the stack is empty again, so a following `}` is a plain
// `BraceClose` rather than the start of a new string fragment.
#[test_case(
    r#""a" }"#
    => r#"Str("a") }"#
    ; "close brace after a complete string"
)]
#[test_case(
    r#""a${b}c" }"#
    => r#"Str("a${) Ident(b) Str(}c") }"#
    ; "close brace after a complete interpolated string"
)]
#[test_case(
    r#"d:m | compute msg = "svc=${svc} code=${code}""#
    => r#"Ident(d) : Ident(m) | Kw(compute) Ident(msg) = Str("svc=${) Ident(svc) Str(} code=${) Ident(code) Str(}")"#
    ; "interpolation in a realistic query"
)]
fn nested_string_interpolation(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Unterminated interpolation
//
// Truncation can land in three different places — inside the marker, inside the body, and
// after the closing `}` — and each leaves the state stack in a different shape. None of
// them may hang or panic; `assert_total` and `assert_tiles` cover that for the same inputs
// via `CORPUS`, and these cases pin down which token the input degrades to.
// ---------------------------------------------------------------------------------------

#[test_case(r#""a${"#     => r#"Str("a${)"#                       ; "ends right after the marker")]
#[test_case(r#""${"#      => r#"Str("${)"#                        ; "ends right after a leading marker")]
#[test_case(r#""a$"#      => r#"Invalid("a$)"#                    ; "ends on a dollar")]
#[test_case(r#""${x"#     => r#"Str("${) Ident(x)"#               ; "ends inside the body")]
#[test_case(r#""${x}"#    => r#"Str("${) Ident(x) Invalid(})"#    ; "ends on the closing brace")]
#[test_case(r#""${x}b"#   => r#"Str("${) Ident(x) Invalid(}b)"#   ; "ends inside the trailing fragment")]
#[test_case(r#""${x}b""#  => r#"Str("${) Ident(x) Str(}b")"#      ; "terminated for contrast")]
#[test_case(r#""a${"b""#  => r#"Str("a${) Str("b")"#              ; "nested literal closes but the outer does not")]
#[test_case(r#""a${"b"#   => r#"Str("a${) Invalid("b)"#           ; "nested literal is itself unterminated")]
fn unterminated_interpolation(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Regexes
// ---------------------------------------------------------------------------------------

#[test_case("#/x/"          => "Regex(#/x/)"            ; "plain")]
#[test_case("#//"           => "Regex(#//)"             ; "empty")]
#[test_case("#/[123]../"    => "Regex(#/[123]../)"      ; "dots inside a regex are regex syntax")]
#[test_case(r"#/a\//"       => r"Regex(#/a\//)"         ; "escaped slash before terminator")]
#[test_case(r"#/a\/\/b/"    => r"Regex(#/a\/\/b/)"      ; "consecutive escaped slashes")]
#[test_case(r"#/a\\/"       => r"Regex(#/a\\/)"         ; "escaped backslash before terminator")]
#[test_case("#abc"          => "Invalid(#) Ident(abc)"  ; "hash without slash")]
#[test_case("#/unterminated" => "Invalid(#/unterminated)" ; "unterminated")]
// One case per member of the accepted escape set, so narrowing the whitelist cannot pass
// unnoticed; `\q` pins down that the set really is a whitelist and not "anything after a
// backslash".
#[test_case(r"#/\{\}\[\]/"  => r"Regex(#/\{\}\[\]/)"    ; "escaped braces and brackets")]
#[test_case(r"#/\(\)\*\./"  => r"Regex(#/\(\)\*\./)"    ; "escaped group repeat and dot")]
#[test_case(r"#/\+\|\$/"    => r"Regex(#/\+\|\$/)"      ; "escaped plus alternation and anchor")]
#[test_case(r"#/\n\t\r/"    => r"Regex(#/\n\t\r/)"      ; "escaped control characters")]
#[test_case(r"#/a\qb/"      => r"Invalid(#/a\) Ident(qb) /" ; "escape outside the whitelist is rejected")]
#[test_case(r"#/a\"         => r"Invalid(#/a\)"         ; "backslash at end of input")]
fn regexes(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Variables
// ---------------------------------------------------------------------------------------

#[test_case("$foo"     => "Var($foo)"                ; "plain")]
#[test_case("$_x1"     => "Var($_x1)"                ; "underscore and digit")]
#[test_case("$`a b`"   => "EscVar($`a b`)"           ; "escaped")]
#[test_case(r"$`a\n`"  => r"EscVar($`a\n`)"          ; "escaped with escape")]
#[test_case("$1"       => "Invalid($) Int(1)"        ; "digit cannot start a variable")]
#[test_case("$"        => "Invalid($)"               ; "bare dollar")]
fn variables(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Comments and whitespace
// ---------------------------------------------------------------------------------------

#[test_case("// hi"        => "Comment(// hi)"                       ; "to end of input")]
#[test_case("a // hi\nb"   => "Ident(a) Comment(// hi) Ident(b)"     ; "comment stops at newline")]
#[test_case("a / b"        => "Ident(a) / Ident(b)"                  ; "single slash is division")]
fn comments(src: &str) -> String {
    lex(src)
}

#[test_case("a b"      => "Ident(a) WS Ident(b)"      ; "single space")]
#[test_case("a  \n\t b" => "Ident(a) WS Ident(b)"     ; "mixed whitespace coalesces")]
#[test_case(" a"       => "WS Ident(a)"               ; "leading")]
#[test_case("a "       => "Ident(a) WS"               ; "trailing")]
fn whitespace(src: &str) -> String {
    lex_ws(src)
}

// ---------------------------------------------------------------------------------------
// Unicode
//
// Identifiers accept Unicode by design; numbers are ASCII-only so that an `Integer` token's
// text always parses. Everything else non-ASCII is rejected rather than panicking, which is
// the part that regressed historically.
// ---------------------------------------------------------------------------------------

#[test_case("öff"           => "Ident(öff)"                      ; "unicode ident start")]
#[test_case("a\u{0967}b"    => "Ident(a\u{0967}b)"               ; "unicode digit continues an ident")]
#[test_case("`föö`"         => "EscIdent(`föö`)"                 ; "unicode escaped ident")]
#[test_case("\"héllo\""     => "Str(\"héllo\")"                  ; "unicode string")]
#[test_case("#/é/"          => "Regex(#/é/)"                     ; "unicode regex")]
#[test_case("// héllo\nfoo" => "Comment(// héllo) WS Ident(foo)" ; "unicode comment")]
#[test_case("€"             => "Invalid(€)"                      ; "unknown multi byte char")]
#[test_case("a 🎉 b"        => "Ident(a) WS Invalid(🎉) WS Ident(b)" ; "emoji")]
#[test_case("\u{00B2}"      => "Invalid(\u{00B2})"               ; "superscript two is not a digit")]
#[test_case("\u{0664}"      => "Invalid(\u{0664})"               ; "arabic indic digit is not a digit")]
#[test_case("a\u{00A0}b"    => "Ident(a) WS Ident(b)"            ; "non breaking space is whitespace")]
#[test_case("\u{3000}foo"   => "WS Ident(foo)"                   ; "ideographic space is whitespace")]
fn unicode(src: &str) -> String {
    lex_ws(src)
}

// ---------------------------------------------------------------------------------------
// Realistic queries
// ---------------------------------------------------------------------------------------

#[test_case(
    "d:m | where code >= 500"
    => "Ident(d) : Ident(m) | Kw(where) Ident(code) >= Int(500)"
    ; "filter with comparison"
)]
#[test_case(
    "d:m[5m..] | group by pod using sum"
    => "Ident(d) : Ident(m) [ Int(5) Ident(m) .. ] | Kw(group) Kw(by) Ident(pod) Kw(using) Ident(sum)"
    ; "time range and group by"
)]
#[test_case(
    "d:m | where tag in [\"a\", 1, 2.3]"
    => "Ident(d) : Ident(m) | Kw(where) Ident(tag) Ident(in) [ Str(\"a\") , Int(1) , Float(2.3) ]"
    ; "in with a mixed array"
)]
#[test_case(
    "param $ds: Dataset; $ds:m | filter svc == #/api-.+/"
    => "Kw(param) Var($ds) : Ident(Dataset) ; Var($ds) : Ident(m) | Kw(filter) Ident(svc) == Regex(#/api-.+/)"
    ; "param declaration and regex filter"
)]
#[test_case(
    "( a:b | compute x using sum, c:d, ) | compute y using /"
    => "( Ident(a) : Ident(b) | Kw(compute) Ident(x) Kw(using) Ident(sum) , Ident(c) : Ident(d) , ) | Kw(compute) Ident(y) Kw(using) /"
    ; "compute query"
)]
fn queries(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Structural properties
//
// These run over every input the tables use plus the shipped examples plus randomly
// generated garbage. They check relationships the tables cannot express, and they are what
// makes the suite resistant to the offset bugs that string comparison alone will not see.
// ---------------------------------------------------------------------------------------

/// Inputs that have historically broken the lexer, kept together so every property below
/// gets the same adversarial corpus.
const CORPUS: &[&str] = &[
    "",
    " ",
    "d:m | where code >= 500",
    "d:m[1700000000..1700003600] | group by pod using sum",
    "300..600",
    "2.",
    "1...2",
    "[2.5..3]",
    "a <= b >= c != d == e",
    r"`a\n`",
    r"`a\``",
    r"`a\\`",
    "`abc",
    "`föö`",
    "\"héllo\"",
    "\"abc",
    r"#/a\//",
    r"#/a\/\/b/",
    "#/unterminated",
    "#abc",
    "// héllo\nfoo",
    "öff",
    "€",
    "a 🎉 b",
    "a\u{00A0}b",
    "\u{3000}foo",
    "\u{00B2}",
    r"$`a\n`",
    "$1",
    // Interpolation: the state stack makes tokenisation position-dependent, so the tiling
    // and prefix-stability properties matter more here than anywhere else in the lexer.
    r#""a${x}b""#,
    r#""${}""#,
    r#""${x}${y}""#,
    r#""a$b""#,
    r#""a\${x}""#,
    r#""a${"b${c}d"}e""#,
    r#""${{a}}""#,
    r#"{ "a${b}c" }"#,
    r#""a${"#,
    r#""${x}"#,
    r#""${"${"}""#,
    r#""héllo ${é}""#,
    "~",
    r"`aA`",
    r"\",
    "\"\"\"",
    "```",
    "###",
    "...",
];

/// Property: the tokens tile the input exactly.
///
/// The first token starts at 0, each subsequent token starts where the previous one ended,
/// and the last one ends at `input.len()`. This single relationship pins down four things
/// that were each broken at some point: offsets are byte-based rather than char-based,
/// every offset lands on a character boundary, no token's text is truncated relative to
/// where the lexer actually stopped, and no input is silently dropped or double-counted.
///
/// It does not catch a correct-length but wrongly-split stream — `<` `=` tiles just as well
/// as `<=`. That is what the tables above are for; the two layers are complementary.
fn assert_tiles(input: &str) {
    let mut cursor = 0usize;
    for token in Lexer::new(input) {
        let (start, kind, text) = parts(&token);

        assert!(
            start <= input.len(),
            "token {kind} start {start} past end of {input:?}"
        );
        assert!(
            input.is_char_boundary(start),
            "token {kind} start {start} is not a char boundary in {input:?}"
        );
        assert_eq!(
            start, cursor,
            "gap or overlap before token {kind} in {input:?}"
        );
        if let Some(text) = text {
            assert!(
                input[start..].starts_with(text),
                "token {kind} text {text:?} does not match input at {start} in {input:?}"
            );
        }

        cursor = start + span_len(&token);
    }
    assert_eq!(
        cursor,
        input.len(),
        "lexer stopped before the end of {input:?}"
    );
}

/// Property: lexing terminates and never panics.
///
/// A lexer that fails to consume a character loops forever; one that mis-slices a multi-byte
/// character panics inside `Index<Range<usize>>`. Both are failure modes this crate has
/// actually shipped, and neither shows up as a wrong token — only as a hang or a crash.
fn assert_total(input: &str) {
    let count = Lexer::new(input).count();
    assert!(
        count <= input.len() + 1,
        "produced {count} tokens for {} bytes of input {input:?}",
        input.len()
    );
}

#[test]
fn corpus_tiles() {
    for input in CORPUS {
        assert_tiles(input);
        assert_total(input);
    }
}

#[test]
fn examples_tile() {
    let dir = std::fs::read_dir("./tests/examples").expect("examples dir");
    let mut checked = 0;
    for entry in dir.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "mpl") {
            continue;
        }
        let content = std::fs::read_to_string(&path).expect("readable example");
        assert_tiles(&content);
        assert_total(&content);
        checked += 1;
    }
    // Guard against the directory being renamed and the test silently passing over nothing,
    // which is how `tests/lex.rs::parse_unimplemented_examples` ended up vacuous.
    assert!(checked > 0, "no .mpl examples found");
}

/// Xorshift64* so a failing case is reproducible from its seed alone. Using a real PRNG
/// crate would pull in a dev-dependency for thirty lines of arithmetic.
struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        let idx = usize::try_from(self.next_u64() % items.len() as u64).unwrap_or(0);
        &items[idx]
    }
}

/// Fragments chosen so the generator produces near-miss MPL rather than uniform noise:
/// delimiters that must be balanced, escapes that must consume a following character, and
/// multi-byte characters in every lexical position.
const FRAGMENTS: &[&str] = &[
    "0", "1", "9", "a", "z", "_", " ", "\n", "\t", ".", "..", ":", "::", ";", ",", "|", "(", ")",
    "[", "]", "{", "}", "<", ">", "=", "!", "?", "+", "-", "*", "/", "//", "\"", "`", "#", "$",
    "\\", "~", "%", "é", "ö", "€", "🎉", "\u{00B2}", "\u{00A0}", "\u{3000}", "where", "1.5",
    // `${` as one fragment rather than relying on `$` and `{` landing next to each other by
    // chance, so interpolation openers appear often enough to interleave with `"` and `}`.
    "${",
];

#[test]
fn generated_inputs_tile() {
    let mut rng = Rng(0x5eed_1234_abcd_ef01);
    for _ in 0..2000 {
        let len = usize::try_from(rng.next_u64() % 20).unwrap_or(0);
        let mut input = String::new();
        for _ in 0..len {
            input.push_str(rng.pick(FRAGMENTS));
        }
        assert_tiles(&input);
        assert_total(&input);
    }
}

/// Property: appending input never changes the tokens already produced for the prefix, up to
/// the one token straddling the boundary.
///
/// The lexer is a single left-to-right pass with at most two characters of lookahead, so a
/// later character must not retroactively change an earlier token. This is what would break
/// if lookahead were ever widened without care — for instance if `..` detection started
/// scanning further ahead.
#[test]
fn prefix_tokens_are_stable() {
    for input in CORPUS {
        if input.is_empty() {
            continue;
        }
        let full: Vec<String> = Lexer::new(input).map(|t| describe(&t)).collect();
        for split in 1..input.len() {
            if !input.is_char_boundary(split) {
                continue;
            }
            let prefix: Vec<String> = Lexer::new(&input[..split]).map(|t| describe(&t)).collect();
            // The final prefix token may be cut short by the split, so compare all but that.
            let shared = prefix.len().saturating_sub(1);
            assert_eq!(
                &prefix[..shared],
                &full[..shared],
                "tokens changed when input was extended past byte {split} of {input:?}"
            );
        }
    }
}

/// Property: whitespace and comments carry no syntactic weight beyond separating tokens.
///
/// Dropping them from the stream must leave the remaining tokens identical, which is the
/// assumption every consumer of this lexer will make when it filters them out.
#[test]
fn filtering_trivia_preserves_the_rest() {
    for input in CORPUS {
        let kept: Vec<String> = Lexer::new(input)
            .filter(|t| !matches!(t, Token::Whitespace(..) | Token::Comment(..)))
            .map(|t| describe(&t))
            .collect();
        let all: Vec<String> = Lexer::new(input)
            .map(|t| describe(&t))
            .filter(|d| d != "WS" && !d.starts_with("Comment("))
            .collect();
        assert_eq!(
            kept, all,
            "filtering trivia changed the stream for {input:?}"
        );
    }
}
