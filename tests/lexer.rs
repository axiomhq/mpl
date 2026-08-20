//! Token-stream tests for the MPL lexer.
//!
//! Cases are written as `source => expected token stream`, where the expected side is a
//! compact rendering: operators and literals render as their own source text (`<=`, `..`,
//! `true`), everything else as `TokenType(text)` using the type's own `Debug` name. Writing
//! the whole stream on one line is what makes a table readable; the alternative — building
//! `Vec<Token>` literals — buries the signal in offsets that are better checked by the
//! structural properties at the bottom of this file.
//!
//! Offsets are deliberately absent from the tables. They are covered exhaustively by
//! `assert_tiles`, which is a stronger check than spot-asserting a few numbers by hand.

use mpl_lang::lexer::{Lexer, Token, TokenType};
use test_case::test_case;

/// Whether a token's own source text is its name. `==` and `..` describe themselves, so
/// rendering them as `EqualEqual(==)` would only add noise; `true`/`false`/`inf` are the same
/// case, the text *is* the literal.
///
/// Only the exceptions are listed — the tokens whose text varies and therefore need naming.
/// Everything else renders as `Debug(text)`, so a new operator or literal costs no edit here
/// and a new named token costs one line.
fn is_self_describing(tpe: TokenType) -> bool {
    !matches!(
        tpe,
        TokenType::Invalid
            | TokenType::Whitespace
            | TokenType::Ident
            | TokenType::EscapedIdent
            | TokenType::Comment
            | TokenType::Integer
            | TokenType::Float
            | TokenType::Variable
            | TokenType::EscapedVariable
            | TokenType::Regex
            | TokenType::String
            | TokenType::StringStart
            | TokenType::StringSegment
            | TokenType::StringEnd
    )
}

/// Renders one token for the expected column.
fn describe(token: &Token<'_>) -> String {
    let tpe = token.tpe();
    if is_self_describing(tpe) {
        return token.text().to_string();
    }
    format!("{tpe:?}({})", token.text())
}

/// Lexes into the compact form, dropping whitespace — the default for the tables below.
fn lex(input: &str) -> String {
    Lexer::new(input)
        .filter(|t| t.tpe() != TokenType::Whitespace && !t.is_eof())
        .map(|t| describe(&t))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lexes into the compact form, keeping whitespace tokens.
fn lex_ws(input: &str) -> String {
    Lexer::new(input)
        .filter(|t| !t.is_eof())
        .map(|t| describe(&t))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------------------
// Operators
//
// The `<>` / `=<` / `=>` cases are regression guards: each was accepted as a single
// comparison token at some point, and none of them is an MPL operator (spec.md, Filter
// Expression).
// ---------------------------------------------------------------------------------------

#[test_case("a == 1" => "Ident(a) == Integer(1)"     ; "equal equal")]
#[test_case("a != 1" => "Ident(a) != Integer(1)"     ; "not equal")]
#[test_case("a = 1"  => "Ident(a) = Integer(1)"      ; "assign")]
#[test_case("a < 1"  => "Ident(a) < Integer(1)"      ; "less than")]
#[test_case("a <= 1" => "Ident(a) <= Integer(1)"     ; "less than equal")]
#[test_case("a > 1"  => "Ident(a) > Integer(1)"      ; "greater than")]
#[test_case("a >= 1" => "Ident(a) >= Integer(1)"     ; "greater than equal")]
#[test_case("a <> 1" => "Ident(a) < > Integer(1)"    ; "diamond is not an operator")]
#[test_case("a =< 1" => "Ident(a) = < Integer(1)"    ; "reversed le is not an operator")]
#[test_case("a => 1" => "Ident(a) = > Integer(1)"    ; "reversed ge is not an operator")]
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

#[test_case("0"        => "Integer(0)"                 ; "zero")]
#[test_case("42"       => "Integer(42)"                ; "integer")]
#[test_case("1.5"      => "Float(1.5)"             ; "float")]
#[test_case("2."       => "Float(2.)"              ; "a trailing dot is part of the float")]
#[test_case("f(2., 3)" => "Ident(f) ( Float(2.) , Integer(3) )" ; "trailing dot before delimiter")]
#[test_case("300..600" => "Integer(300) .. Integer(600)"   ; "timestamp range")]
#[test_case("1..2"     => "Integer(1) .. Integer(2)"       ; "single digit range")]
#[test_case("[2.5..3]" => "[ Float(2.5) .. Integer(3) ]" ; "float then range")]
#[test_case("[1..2.5]" => "[ Integer(1) .. Float(2.5) ]" ; "range then float")]
#[test_case("[5m..]"   => "[ Integer(5) Ident(m) .. ]" ; "relative open ended range")]
#[test_case("[..600]"  => "[ .. Integer(600) ]"        ; "open start range")]
#[test_case("1700000000..1700003600" => "Integer(1700000000) .. Integer(1700003600)" ; "unix timestamp range")]
#[test_case("1...2"    => "Integer(1) .. Invalid(.) Integer(2)" ; "three dots is not valid syntax")]
#[test_case("a . b"    => "Ident(a) Invalid(.) Ident(b)" ; "lone dot is invalid")]
fn numbers_and_ranges(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Identifiers
//
// The lexer emits no keywords at all: every word-shaped thing is an `Ident` and the parser
// decides what it means. Deciding here would need to know grammar position — whether `filter`
// sits in a pipeline verb, a module path or a tag slot — and that is parser state. Paying for
// it would buy nothing, because the parser matches the text either way.
//
// `filter` and `is` are why the alternative cannot be patched into working: each is both a
// a rule name and a stdlib module (src/stdlib.rs), and a function path is
// `(module ::)* ident` (`Parser::function_path`), so there is no keyword reading of
// `map filter::gt(1)` — a query that ships in tests/examples/map-gt.mpl:2.
// ---------------------------------------------------------------------------------------

#[test_case("foo"        => "Ident(foo)"                 ; "plain")]
#[test_case("_foo"       => "Ident(_foo)"                ; "leading underscore")]
#[test_case("foo_bar1"   => "Ident(foo_bar1)"            ; "digits and underscores")]
#[test_case("where"      => "Ident(where)"               ; "grammar keyword is a plain ident")]
#[test_case("group by"   => "Ident(group) Ident(by)"     ; "two grammar keywords")]
#[test_case("wherever"   => "Ident(wherever)"            ; "maximal munch over a keyword prefix")]
#[test_case("sum"        => "Ident(sum)"                 ; "stdlib function name")]
#[test_case("filter::gt" => "Ident(filter) :: Ident(gt)" ; "module path a keyword would break")]
#[test_case("is::lt"     => "Ident(is) :: Ident(lt)"     ; "second module path a keyword would break")]
fn identifiers(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Value literals
//
// `true`, `false` and `inf` are the only words the lexer resolves, because they are values
// rather than names — `inf` is a number (spec.md, Float). They therefore behave like the
// numeric tokens, sign included: `+inf` splits its sign exactly as `-5` lexes as `-`
// `Integer(5)`.
//
// The lexer takes the word boundary from maximal munch, so the cases worth having are the
// ones where a longer ident starts with a literal.
// ---------------------------------------------------------------------------------------

#[test_case("true"           => "true"                    ; "true literal")]
#[test_case("false"          => "false"                   ; "false literal")]
#[test_case("inf"            => "inf"                     ; "inf literal")]
#[test_case("+inf"           => "+ inf"                   ; "sign splits off, as it does for ints")]
#[test_case("-inf"           => "- inf"                   ; "negative sign splits off too")]
#[test_case("infinity"       => "Ident(infinity)"         ; "longer ident starting with inf")]
#[test_case("trueish"        => "Ident(trueish)"          ; "longer ident starting with true")]
#[test_case("inf_"           => "Ident(inf_)"             ; "underscore continues the ident")]
#[test_case("Inf"            => "Ident(Inf)"              ; "inf is case sensitive")]
#[test_case("True"           => "Ident(True)"             ; "bools are case sensitive")]
#[test_case("[1, true, inf]" => "[ Integer(1) , true , inf ]" ; "literals inside an array")]
#[test_case("\"true\""       => "String(\"true\")"           ; "inside a string it stays text")]
fn value_literals(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Escaped identifiers
//
// The escape arm must consume exactly the backslash and the escaped character. Consuming
// one char too many silently swallowed the closing delimiter, which is why every case here
// puts an escape immediately before the terminator.
// ---------------------------------------------------------------------------------------

#[test_case("`foo`"    => "EscapedIdent(`foo`)"      ; "plain")]
#[test_case("``"       => "EscapedIdent(``)"         ; "empty")]
#[test_case("`a b`"    => "EscapedIdent(`a b`)"      ; "with space")]
#[test_case(r"`a\n`"   => r"EscapedIdent(`a\n`)"     ; "escape before terminator")]
#[test_case(r"`a\nb`"  => r"EscapedIdent(`a\nb`)"    ; "escape mid literal")]
#[test_case(r"`a\``"   => r"EscapedIdent(`a\``)"     ; "escaped backtick before terminator")]
#[test_case(r"`a\\`"   => r"EscapedIdent(`a\\`)"     ; "escaped backslash before terminator")]
#[test_case(r"`a\t\r`" => r"EscapedIdent(`a\t\r`)"   ; "consecutive escapes")]
#[test_case("`abc"     => "Invalid(`abc)"        ; "unterminated")]
fn escaped_identifiers(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Strings
//
// A string literal and an escaped identifier are different things (spec.md, String and
// Identifiers), so they must not collapse to the same token kind.
// ---------------------------------------------------------------------------------------

#[test_case("\"foo\""       => "String(\"foo\")"                  ; "plain")]
#[test_case("\"\""          => "String(\"\")"                     ; "empty")]
#[test_case("\"foo\" `foo`" => "String(\"foo\") EscapedIdent(`foo`)"  ; "string is distinct from escaped ident")]
#[test_case("\"abc"         => "Invalid(\"abc)"                ; "unterminated")]
fn strings(src: &str) -> String {
    lex(src)
}

// One case per member of `parse_string`'s escape whitelist, so narrowing it cannot pass
// unnoticed. Each escape sits immediately before the closing quote: an escape arm that
// consumes one character too many swallows the terminator, which is the failure mode the
// escaped-identifier tests above were written for.
#[test_case(r#""say \"hi\"""# => r#"String("say \"hi\"")"# ; "escaped quote")]
#[test_case(r#""a\\""#        => r#"String("a\\")"#        ; "escaped backslash")]
#[test_case(r#""a\n\t\r""#    => r#"String("a\n\t\r")"#    ; "escaped control characters")]
#[test_case(r#""a\b\f""#      => r#"String("a\b\f")"#      ; "escaped backspace and form feed")]
#[test_case(r#""a\"#          => r#"Invalid("a\)"#      ; "backslash at end of input")]
fn string_escapes(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// String interpolation
//
// `${` does not get a token of its own: it terminates the fragment that precedes it and is
// included in that fragment's text, and the matching `}` opens the fragment that follows
// it. So `"a${x}b"` is three tokens — `StringStart("a${)`, `Ident(x)`, `StringEnd(}b")` — and
// a consumer reassembles the literal by stripping the `${` / `}` markers from the fragments.
//
// The fragment's *kind* is its position in the literal: `StringStart` opens one and promises
// a body, `StringSegment` sits between two bodies and promises another, `StringEnd` closes
// the literal, and `String` is a literal with no interpolation at all. `Parser::string`
// (src/syntax_tree.rs:647) walks exactly that sequence, so every case below pins which kind
// each fragment is — a segment mistyped as a `StringEnd` terminates the literal one
// interpolation early.
//
// The tests are written against that shape deliberately: the invariant worth pinning is
// that the interpolation markers stay attached to the string fragments and that the body
// between them is lexed as ordinary MPL, not as string content.
//
// The competing reading of `$` is a variable (src/lexer.rs:355-376), so every case where a
// `$` is *not* followed by `{` must stay inside the literal.
// ---------------------------------------------------------------------------------------

#[test_case(r#""${x}""#        => r#"StringStart("${) Ident(x) StringEnd(}")"#   ; "whole string is one interpolation")]
#[test_case(r#""a${x}b""#      => r#"StringStart("a${) Ident(x) StringEnd(}b")"# ; "text on both sides")]
#[test_case(r#""${x}b""#       => r#"StringStart("${) Ident(x) StringEnd(}b")"#  ; "text after only")]
#[test_case(r#""a${x}""#       => r#"StringStart("a${) Ident(x) StringEnd(}")"#  ; "text before only")]
#[test_case(r#""${}""#         => r#"StringStart("${) StringEnd(}")"#            ; "empty interpolation")]
#[test_case(r#""a${x}b${y}c""# => r#"StringStart("a${) Ident(x) StringSegment(}b${) Ident(y) StringEnd(}c")"# ; "two interpolations")]
#[test_case(r#""${x}${y}""#    => r#"StringStart("${) Ident(x) StringSegment(}${) Ident(y) StringEnd(}")"# ; "adjacent interpolations")]
// The body is a token stream, not a substring: operators, keywords, numbers, variables and
// regexes all lex as themselves inside `${ }`.
#[test_case(r#""${a + b}""#    => r#"StringStart("${) Ident(a) + Ident(b) StringEnd(}")"#              ; "expression body")]
#[test_case(r#""${a:b}""#      => r#"StringStart("${) Ident(a) : Ident(b) StringEnd(}")"#              ; "colon in body")]
#[test_case(r#""${$foo}""#     => r#"StringStart("${) Variable($foo) StringEnd(}")"#                   ; "variable body")]
#[test_case(r#""${where}""#    => r#"StringStart("${) Ident(where) StringEnd(}")"#                     ; "grammar keyword body")]
#[test_case(r#""${1.5}""#      => r#"StringStart("${) Float(1.5) StringEnd(}")"#                       ; "float body")]
#[test_case(r#""${#/a/}""#     => r#"StringStart("${) Regex(#/a/) StringEnd(}")"#                      ; "regex body")]
#[test_case(r#""${f(a, b)}""#  => r#"StringStart("${) Ident(f) ( Ident(a) , Ident(b) ) StringEnd(}")"# ; "call body")]
// A `$` only opens an interpolation when `{` follows it; everything else stays literal.
#[test_case(r#""a$b""#         => r#"String("a$b")"#                                ; "dollar before an ident is literal")]
#[test_case(r#""a$""#          => r#"String("a$")"#                                 ; "dollar before the terminator is literal")]
#[test_case(r#""$""#           => r#"String("$")"#                                  ; "lone dollar is literal")]
#[test_case(r#""$$x""#         => r#"String("$$x")"#                                ; "double dollar is literal")]
#[test_case(r#""a\${x}""#      => r#"String("a\${x}")"#                             ; "escaped dollar suppresses interpolation")]
#[test_case(r#""héllo ${x}""#  => r#"StringStart("héllo ${) Ident(x) StringEnd(}")"# ; "multi byte text before an interpolation")]
#[test_case(r#""${é}""#        => r#"StringStart("${) Ident(é) StringEnd(}")"#       ; "multi byte ident in the body")]
fn string_interpolation(src: &str) -> String {
    lex(src)
}

/// Whitespace inside `${ }` is a `Whitespace` token, not string content — the clearest
/// single demonstration that the body leaves string-literal mode entirely.
#[test_case(r#""${ x }""#   => r#"StringStart("${) Whitespace( ) Ident(x) Whitespace( ) StringEnd(}")"# ; "spaces around the body")]
#[test_case(r#""a b${ c }""# => r#"StringStart("a b${) Whitespace( ) Ident(c) Whitespace( ) StringEnd(}")"# ; "spaces in the literal stay in the fragment")]
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
    => r#"StringStart("${) String("b") StringEnd(}")"#
    ; "string literal inside an interpolation"
)]
#[test_case(
    r#""a${"b"}c""#
    => r#"StringStart("a${) String("b") StringEnd(}c")"#
    ; "string literal inside an interpolation with surrounding text"
)]
#[test_case(
    r#""a${"b${c}d"}e""#
    => r#"StringStart("a${) StringStart("b${) Ident(c) StringEnd(}d") StringEnd(}e")"#
    ; "interpolation inside an interpolated string"
)]
#[test_case(
    r#""a${"b${"c${d}e"}f"}g""#
    => r#"StringStart("a${) StringStart("b${) StringStart("c${) Ident(d) StringEnd(}e") StringEnd(}f") StringEnd(}g")"#
    ; "three levels deep"
)]
#[test_case(
    r#""${"a" + "b"}""#
    => r#"StringStart("${) String("a") + String("b") StringEnd(}")"#
    ; "two sibling strings in one body"
)]
// A `}` inside a nested literal is string content, so it must not pop the outer `StrOpen`
// and end the interpolation early.
#[test_case(
    r#""${"}"}""#
    => r#"StringStart("${) String("}") StringEnd(}")"#
    ; "close brace inside a nested literal is content"
)]
#[test_case(
    r#""${"${"}""#
    => r#"StringStart("${) StringStart("${) String("}")"#
    ; "dollar brace inside a nested literal opens another level"
)]
// Braces and interpolations interleaved: a `{` pushed inside a body must be popped by its
// own `}` before the interpolation's `}` is reached.
#[test_case(
    r#""${{a}}""#
    => r#"StringStart("${) { Ident(a) } StringEnd(}")"#
    ; "brace group inside a body"
)]
#[test_case(
    r#""${ {a: "x"} }""#
    => r#"StringStart("${) { Ident(a) : String("x") } StringEnd(}")"#
    ; "brace group containing a string"
)]
#[test_case(
    r#"{ "a${b}c" }"#
    => r#"{ StringStart("a${) Ident(b) StringEnd(}c") }"#
    ; "interpolated string inside a brace group"
)]
#[test_case(
    r#"{ "a${ {b} }c" }"#
    => r#"{ StringStart("a${) { Ident(b) } StringEnd(}c") }"#
    ; "brace group inside an interpolation inside a brace group"
)]
// Once a literal is closed the stack is empty again, so a following `}` is a plain
// `BraceClose` rather than the start of a new string fragment.
#[test_case(
    r#""a" }"#
    => r#"String("a") }"#
    ; "close brace after a complete string"
)]
#[test_case(
    r#""a${b}c" }"#
    => r#"StringStart("a${) Ident(b) StringEnd(}c") }"#
    ; "close brace after a complete interpolated string"
)]
#[test_case(
    r#"d:m | compute msg = "svc=${svc} code=${code}""#
    => r#"Ident(d) : Ident(m) | Ident(compute) Ident(msg) = StringStart("svc=${) Ident(svc) StringSegment(} code=${) Ident(code) StringEnd(}")"#
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

#[test_case(r#""a${"#     => r#"StringStart("a${)"#                     ; "ends right after the marker")]
#[test_case(r#""${"#      => r#"StringStart("${)"#                      ; "ends right after a leading marker")]
#[test_case(r#""a$"#      => r#"Invalid("a$)"#                            ; "ends on a dollar")]
#[test_case(r#""${x"#     => r#"StringStart("${) Ident(x)"#             ; "ends inside the body")]
#[test_case(r#""${x}"#    => r#"StringStart("${) Ident(x) Invalid(})"#  ; "ends on the closing brace")]
#[test_case(r#""${x}b"#   => r#"StringStart("${) Ident(x) Invalid(}b)"# ; "ends inside the trailing fragment")]
#[test_case(r#""${x}b""#  => r#"StringStart("${) Ident(x) StringEnd(}b")"# ; "terminated for contrast")]
#[test_case(r#""a${"b""#  => r#"StringStart("a${) String("b")"#         ; "nested literal closes but the outer does not")]
#[test_case(r#""a${"b"#   => r#"StringStart("a${) Invalid("b)"#         ; "nested literal is itself unterminated")]
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
#[test_case(r"#/a\"         => r"Invalid(#/a\)"         ; "backslash at end of input")]
fn regexes(src: &str) -> String {
    lex(src)
}

// ---------------------------------------------------------------------------------------
// Variables
// ---------------------------------------------------------------------------------------

#[test_case("$foo"     => "Variable($foo)"                ; "plain")]
#[test_case("$_x1"     => "Variable($_x1)"                ; "underscore and digit")]
#[test_case("$`a b`"   => "EscapedVariable($`a b`)"           ; "escaped")]
#[test_case(r"$`a\n`"  => r"EscapedVariable($`a\n`)"          ; "escaped with escape")]
#[test_case("$1"       => "Invalid($) Integer(1)"        ; "digit cannot start a variable")]
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

#[test_case("a b"      => "Ident(a) Whitespace( ) Ident(b)"      ; "single space")]
#[test_case("a  \n\t b" => "Ident(a) Whitespace(  \n\t ) Ident(b)" ; "mixed whitespace coalesces")]
#[test_case(" a"       => "Whitespace( ) Ident(a)"               ; "leading")]
#[test_case("a "       => "Ident(a) Whitespace( )"               ; "trailing")]
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
#[test_case("`föö`"         => "EscapedIdent(`föö`)"                 ; "unicode escaped ident")]
#[test_case("\"héllo\""     => "String(\"héllo\")"                  ; "unicode string")]
#[test_case("#/é/"          => "Regex(#/é/)"                     ; "unicode regex")]
#[test_case("// héllo\nfoo" => "Comment(// héllo) Whitespace(\n) Ident(foo)" ; "unicode comment")]
#[test_case("€"             => "Invalid(€)"                      ; "unknown multi byte char")]
#[test_case("a 🎉 b"        => "Ident(a) Whitespace( ) Invalid(🎉) Whitespace( ) Ident(b)" ; "emoji")]
#[test_case("\u{00B2}"      => "Invalid(\u{00B2})"               ; "superscript two is not a digit")]
#[test_case("\u{0664}"      => "Invalid(\u{0664})"               ; "arabic indic digit is not a digit")]
#[test_case("a\u{00A0}b"    => "Ident(a) Whitespace(\u{00A0}) Ident(b)"     ; "non breaking space is whitespace")]
#[test_case("\u{3000}foo"   => "Whitespace(\u{3000}) Ident(foo)"            ; "ideographic space is whitespace")]
fn unicode(src: &str) -> String {
    lex_ws(src)
}

// ---------------------------------------------------------------------------------------
// Realistic queries
// ---------------------------------------------------------------------------------------

#[test_case(
    "d:m | where code >= 500"
    => "Ident(d) : Ident(m) | Ident(where) Ident(code) >= Integer(500)"
    ; "filter with comparison"
)]
#[test_case(
    "d:m[5m..] | group by pod using sum"
    => "Ident(d) : Ident(m) [ Integer(5) Ident(m) .. ] | Ident(group) Ident(by) Ident(pod) Ident(using) Ident(sum)"
    ; "time range and group by"
)]
#[test_case(
    "d:m | where tag in [\"a\", 1, 2.3]"
    => "Ident(d) : Ident(m) | Ident(where) Ident(tag) Ident(in) [ String(\"a\") , Integer(1) , Float(2.3) ]"
    ; "in with a mixed array"
)]
#[test_case(
    "param $ds: Dataset; $ds:m | filter svc == #/api-.+/"
    => "Ident(param) Variable($ds) : Ident(Dataset) ; Variable($ds) : Ident(m) | Ident(filter) Ident(svc) == Regex(#/api-.+/)"
    ; "param declaration and regex filter"
)]
#[test_case(
    "d:m | map filter::gt(1) | map is::lt(0.4)"
    => "Ident(d) : Ident(m) | Ident(map) Ident(filter) :: Ident(gt) ( Integer(1) ) | Ident(map) Ident(is) :: Ident(lt) ( Float(0.4) )"
    ; "stdlib module paths that a keyword set would break"
)]
#[test_case(
    "( a:b | compute x using sum, c:d, ) | compute y using /"
    => "( Ident(a) : Ident(b) | Ident(compute) Ident(x) Ident(using) Ident(sum) , Ident(c) : Ident(d) , ) | Ident(compute) Ident(y) Ident(using) /"
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
    // Value literals: `Inf`/`Bool` carry no text through `parts`, so their span length comes
    // from the kind string — `assert_tiles` is what proves that coupling still holds.
    "inf",
    "+inf",
    "true",
    "infinity",
    "[1, true, inf]",
    "filter::gt",
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
///
/// The cursor advances by `Token::end()` rather than by a length the test computes itself, so
/// this also holds the production accessor to the input it claims to describe.
fn assert_tiles(input: &str) {
    let mut cursor = 0usize;
    for token in Lexer::new(input) {
        if token.is_eof() {
            break;
        }

        let start = token.pos();
        let tpe = token.tpe();
        let text = token.text();

        assert!(
            start <= input.len(),
            "token {tpe:?} start {start} past end of {input:?}"
        );
        assert!(
            input.is_char_boundary(start),
            "token {tpe:?} start {start} is not a char boundary in {input:?}"
        );
        assert_eq!(
            start, cursor,
            "gap or overlap before token {tpe:?} in {input:?}"
        );
        assert!(
            input[start..].starts_with(text),
            "token {tpe:?} text {text:?} does not match input at {start} in {input:?}"
        );

        cursor = token.end();
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

/// Property: a fragment's kind is exactly what its own first and last characters say it is.
///
/// Each kind is a promise about what surrounds it: `StringStart` opens the literal and
/// promises a body, `StringSegment` follows a body and promises another, `StringEnd` closes
/// the literal, and `String` stands alone. `Parser::string` (src/syntax_tree.rs:647) walks
/// that sequence on the kinds alone, so they must never be interchangeable: a `${`-terminated
/// fragment typed as `StringEnd` ends the literal an interpolation early, and a
/// `"`-terminated one typed as `StringSegment` sends the parser looking for a body that does
/// not exist.
///
/// Stated over the text rather than over the input, this holds for truncated input too — the
/// lexer degrades a cut-off literal to `Invalid`, which this property deliberately says
/// nothing about.
///
/// Returns how many fragments promised a body, so callers can prove the property was not
/// vacuous — over random input a generator that never emits a well-formed `${` would satisfy
/// this trivially.
fn assert_fragment_kinds(input: &str) -> usize {
    let mut opened = 0;
    for token in Lexer::new(input) {
        let text = token.text();
        match token.tpe() {
            TokenType::StringStart => {
                assert!(
                    text.starts_with('"') && text.ends_with("${"),
                    "StringStart {text:?} does not open a literal and promise a body in {input:?}"
                );
                opened += 1;
            }
            TokenType::StringSegment => {
                assert!(
                    text.starts_with('}') && text.ends_with("${"),
                    "StringSegment {text:?} does not sit between two bodies in {input:?}"
                );
                opened += 1;
            }
            TokenType::StringEnd => assert!(
                text.starts_with('}') && text.ends_with('"'),
                "StringEnd {text:?} does not close a literal in {input:?}"
            ),
            TokenType::String => assert!(
                text.starts_with('"') && text.ends_with('"') && text.len() > 1,
                "String {text:?} is not a closed literal in {input:?}"
            ),
            _ => {}
        }
    }
    opened
}

#[test]
fn corpus_tiles() {
    let mut opened = 0;
    for input in CORPUS {
        assert_tiles(input);
        assert_total(input);
        opened += assert_fragment_kinds(input);
    }
    assert!(opened > 0, "no interpolation markers in the corpus");
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
        assert_fragment_kinds(&content);
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
    "\\", "~", "%", "é", "ö", "€", "🎉", "\u{00B2}", "\u{00A0}", "\u{3000}", "where", "1.5", "inf",
    "true",
    // `${` as one fragment rather than relying on `$` and `{` landing next to each other by
    // chance, so interpolation openers appear often enough to interleave with `"` and `}`.
    "${",
];

#[test]
fn generated_inputs_tile() {
    let mut rng = Rng(0x5eed_1234_abcd_ef01);
    let mut opened = 0;
    for _ in 0..2000 {
        let len = usize::try_from(rng.next_u64() % 20).unwrap_or(0);
        let mut input = String::new();
        for _ in 0..len {
            input.push_str(rng.pick::<&str>(FRAGMENTS));
        }
        assert_tiles(&input);
        assert_total(&input);
        opened += assert_fragment_kinds(&input);
    }
    assert!(
        opened > 0,
        "generator never produced a well-formed interpolation marker"
    );
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
        let full: Vec<String> = Lexer::new(input)
            .filter(|t| !t.is_eof())
            .map(|t| describe(&t))
            .collect();
        for split in 1..input.len() {
            if !input.is_char_boundary(split) {
                continue;
            }
            let prefix: Vec<String> = Lexer::new(&input[..split])
                .filter(|t| !t.is_eof())
                .map(|t| describe(&t))
                .collect();
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
            .filter(|t| !matches!(t.tpe(), TokenType::Whitespace | TokenType::Comment))
            .map(|t| describe(&t))
            .collect();
        let all: Vec<String> = Lexer::new(input)
            .map(|t| describe(&t))
            .filter(|d| !d.starts_with("Whitespace(") && !d.starts_with("Comment("))
            .collect();
        assert_eq!(
            kept, all,
            "filtering trivia changed the stream for {input:?}"
        );
    }
}

/// Property: `is_valid` and `is_invalid` agree with the token type, and with each other.
///
/// These are how a consumer decides whether lexing succeeded — `tests/lex.rs` gates every
/// shipped example on `Token::is_invalid`, so if it ever disagreed with the type, a lexer
/// error would pass as a clean parse and the gate would be silently vacuous.
#[test]
fn validity_agrees_with_the_token_type() {
    for input in CORPUS {
        for token in Lexer::new(input) {
            let invalid = token.tpe() == TokenType::Invalid;
            assert_eq!(
                token.is_invalid(),
                invalid,
                "is_invalid disagrees with {:?} in {input:?}",
                token.tpe()
            );
            assert_eq!(
                token.is_valid(),
                !invalid,
                "is_valid disagrees with {:?} in {input:?}",
                token.tpe()
            );
        }
    }
}
