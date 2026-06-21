//! Flat, trivia-preserving lexer for highlighting, built on `winnow`.
//!
//! Unlike the structural parser ([`super`]), this layer never fails on
//! malformed or incomplete input: the final `any` branch consumes one
//! character as [`HlKind::Unknown`], so the loop always makes progress and
//! always returns a token list. That is exactly what an editor needs to
//! highlight mid-edit text (`metric:cpu | filter region == `).
//!
//! It is intentionally *flat* (token-level, not grammar-aware): keywords and
//! types are classified by a word table, mirroring the regex tables that used
//! to live in `language.ts`. This trades a little precision (a tag literally
//! named `filter` is coloured as a keyword) for robustness and a single
//! source of truth in Rust.
//!
//! The one exception to "flat" is double-quoted strings: the lexer descends
//! into `${ … }` interpolation and re-lexes the embedded expression with the
//! same classifier, so a `$param` inside a literal highlights as a variable and
//! a number as a number. This mirrors how `grammar::string_expr` splits the
//! literal into fragments; the `${`/`}` delimiters carry no colour and are
//! emitted as trivia, so the meaningful stream is fragment / expr / fragment.

use winnow::{
    Parser,
    ascii::{digit0, digit1, multispace1, till_line_ending},
    combinator::{alt, not, opt, peek, preceded, repeat},
    error::ContextError,
    stream::{LocatingSlice, Location},
    token::{any, literal, none_of, one_of, take_while},
};

use super::{HlKind, HlToken};

type LexIn<'s> = LocatingSlice<&'s str>;
type LexResult<T> = winnow::Result<T, ContextError>;

/// Every word the editor renders as a keyword. Mirrors the old
/// `language.ts` `MPL_KEYWORDS` regex (plus `sample`, which the pest-driven
/// tokeniser emitted) so highlighting does not visibly change for users.
const KEYWORDS: &[&str] = &[
    "filter",
    "where",
    "map",
    "group",
    "by",
    "using",
    "align",
    "to",
    "over",
    "from",
    "bucket",
    "join",
    "compute",
    "set",
    "replace",
    "as",
    "extend",
    "and",
    "or",
    "not",
    "is",
    "param",
    "ifdef",
    "else",
    "sample",
    "rate",
    "increase",
    "histogram",
    "interpolate_delta_histogram",
    "interpolate_cumulative_histogram",
    "count",
    "avg",
    "sum",
    "min",
    "max",
];

/// Every word the editor renders as a type. Mirrors `language.ts` `TYPE_RE`,
/// minus the lowercase `dataset`/`metric`/`regex` aliases: those are not real
/// MPL types and miscolour the extremely common `ds:metric` metric name. The
/// legacy lowercase `duration` *is* a valid param type, so it stays.
const TYPES: &[&str] = &[
    "Option", "Dataset", "Duration", "Regex", "Metric", "string", "int", "float", "bool",
    "duration",
];

/// Tokenise `src` for highlighting. Always returns a token list (including
/// trivia: whitespace and comments), even for incomplete or invalid input.
#[must_use]
pub fn highlight(src: &str) -> Vec<HlToken> {
    let mut input = LocatingSlice::new(src);
    let mut tokens = Vec::new();
    loop {
        // Strings are the one construct that yields more than one token: the
        // lexer descends into `${ … }` (see `lex_string`). Everything else is
        // one token per `single_token`, which only fails at EOF (every other
        // path consumes >= 1 char), so this terminates and never panics.
        if peek_lit(&mut input, "\"") {
            lex_string(&mut input, &mut tokens);
            continue;
        }
        match single_token(&mut input) {
            Ok(token) => tokens.push(token),
            Err(_) => break,
        }
    }
    tokens
}

/// Classify a single, non-string token. Strings are handled by [`lex_string`]
/// (top level and inside interpolation), so this never needs a string branch.
fn single_token(input: &mut LexIn) -> LexResult<HlToken> {
    // `alt` tuples cap at 10 branches in winnow, so the branches are grouped.
    alt((
        alt((
            line_comment.value(HlKind::Comment),
            multispace1.value(HlKind::Whitespace),
            regex_token.value(HlKind::Regexp),
            number_token.value(HlKind::Number),
            param_token.value(HlKind::Variable),
        )),
        alt((
            escaped_ident_token.value(HlKind::Variable),
            ident_token,
            cmp_op.value(HlKind::Operator),
            literal("|").value(HlKind::Punctuation),
            punctuation.value(HlKind::Punctuation),
            any.value(HlKind::Unknown),
        )),
    ))
    .with_span()
    .map(|(kind, span)| HlToken {
        start: span.start,
        end: span.end,
        kind,
    })
    .parse_next(input)
}

/// Lex a double-quoted string, descending into `${ … }` interpolation. The
/// caller guarantees the next char is `"`; this always makes progress (it
/// consumes at least the opening quote) and tolerates an unterminated literal.
///
/// Emits one `String` token per literal run — the opening run carries the
/// opening quote, the closing run the closing quote — with each `${ … }`
/// re-lexed by the *same* classifier as top-level code (so `$h` inside a literal
/// is a `Variable`, a number a `Number`, …). This mirrors the fragment split in
/// `grammar::string_expr`; the interpolation delimiters carry no colour and are
/// emitted as trivia, so the meaningful highlight stream is
/// fragment / expr / fragment, not one opaque `String`.
fn lex_string(input: &mut LexIn, out: &mut Vec<HlToken>) {
    let mut run_start = input.current_token_start();
    // The opening quote begins the first literal run.
    let _ = eat(input, "\"");
    loop {
        let _ = string_run_body(input);
        let stop = input.current_token_start();
        // Closing quote: the final run carries it; the literal is complete.
        if eat(input, "\"") {
            push_token(out, run_start, input.current_token_start(), HlKind::String);
            return;
        }
        // `${ expr }` interpolation.
        if eat(input, "${") {
            if stop > run_start {
                push_token(out, run_start, stop, HlKind::String);
            }
            push_token(out, stop, input.current_token_start(), HlKind::Whitespace);
            lex_interpolation_body(input, out);
            let brace = input.current_token_start();
            if eat(input, "}") {
                push_token(out, brace, input.current_token_start(), HlKind::Whitespace);
            }
            run_start = input.current_token_start();
            continue;
        }
        // Neither closing quote nor `${`: EOF. Emit the trailing run, if any.
        if stop > run_start {
            push_token(out, run_start, stop, HlKind::String);
        }
        return;
    }
}

/// Lex the expression inside `${ … }` with the top-level classifier, stopping
/// before the closing `}` (which the caller consumes). A nested string literal
/// recurses through [`lex_string`].
fn lex_interpolation_body(input: &mut LexIn, out: &mut Vec<HlToken>) {
    loop {
        if peek_lit(input, "}") {
            return;
        }
        if peek_lit(input, "\"") {
            lex_string(input, out);
            continue;
        }
        match single_token(input) {
            Ok(token) => out.push(token),
            Err(_) => return,
        }
    }
}

/// Consume one literal run inside a double-quoted string: escape-aware text up
/// to (but not including) the closing quote, an interpolation opener `${`, or
/// EOF. Mirrors the run logic in `string_expr` (a lone `$` not followed by `{`
/// is literal text).
fn string_run_body(input: &mut LexIn) -> LexResult<()> {
    repeat::<_, _, (), _, _>(
        0..,
        alt((
            preceded(literal("\\"), any).void(),
            (not(literal("\"")), not(literal("${")), any).void(),
        )),
    )
    .parse_next(input)
}

/// Push a token spanning `start..end`. Callers guarantee `end > start`.
fn push_token(out: &mut Vec<HlToken>, start: usize, end: usize, kind: HlKind) {
    out.push(HlToken { start, end, kind });
}

/// Consume `lit` if present, reporting whether it matched.
fn eat<'s>(input: &mut LexIn<'s>, lit: &'static str) -> bool {
    let r: LexResult<&'s str> = literal(lit).parse_next(input);
    r.is_ok()
}

/// Whether `lit` is next, without consuming it.
fn peek_lit<'s>(input: &mut LexIn<'s>, lit: &'static str) -> bool {
    let r: LexResult<&'s str> = peek(literal(lit)).parse_next(input);
    r.is_ok()
}

fn line_comment(input: &mut LexIn) -> LexResult<()> {
    (literal("//"), till_line_ending).void().parse_next(input)
}

/// A backtick-escaped identifier. Tolerates an unterminated literal.
fn escaped_ident_token(input: &mut LexIn) -> LexResult<()> {
    literal("`").parse_next(input)?;
    delimited_body('`', input)?;
    opt(literal("`")).void().parse_next(input)
}

/// Consume characters until `delim` or EOF, treating `\x` as a two-char escape
/// so an escaped delimiter does not close the literal.
fn delimited_body(delim: char, input: &mut LexIn) -> LexResult<()> {
    repeat(
        0..,
        alt((
            preceded(literal("\\"), any).void(),
            none_of([delim, '\\']).void(),
        )),
    )
    .parse_next(input)
}

/// `#/regex/` or `#s/src/dst/`. Tolerates a missing closing `/` for mid-edit
/// input. Backtracks (leaving `#` for punctuation) when not actually a regex.
fn regex_token(input: &mut LexIn) -> LexResult<()> {
    literal("#").parse_next(input)?;
    let replace = opt(literal("s")).parse_next(input)?.is_some();
    literal("/").parse_next(input)?;
    regex_body(input)?;
    opt(literal("/")).void().parse_next(input)?;
    if replace {
        regex_body(input)?;
        opt(literal("/")).void().parse_next(input)?;
    }
    Ok(())
}

fn regex_body(input: &mut LexIn) -> LexResult<()> {
    repeat(
        0..,
        alt((
            preceded(literal("\\"), any).void(),
            none_of(['/', '\\']).void(),
        )),
    )
    .parse_next(input)
}

/// `123`, `1.5`, `1e9`, and relative times like `5m` / `30ms`.
fn number_token(input: &mut LexIn) -> LexResult<()> {
    digit1.parse_next(input)?;
    opt(preceded(literal("."), digit0)).parse_next(input)?;
    opt((one_of(['e', 'E']), opt(one_of(['+', '-'])), digit1)).parse_next(input)?;
    opt(time_unit).parse_next(input)?;
    Ok(())
}

fn time_unit(input: &mut LexIn) -> LexResult<()> {
    alt((
        literal("ms").void(),
        one_of(['s', 'm', 'h', 'd', 'w', 'M', 'y']).void(),
    ))
    .parse_next(input)
}

fn param_token(input: &mut LexIn) -> LexResult<()> {
    literal("$").parse_next(input)?;
    word.void().parse_next(input)
}

fn ident_token(input: &mut LexIn) -> LexResult<HlKind> {
    word.parse_next(input).map(classify_word)
}

/// A plain identifier: an alpha/underscore start followed by word characters.
fn word<'s>(input: &mut LexIn<'s>) -> LexResult<&'s str> {
    (
        one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_'),
    )
        .take()
        .parse_next(input)
}

fn classify_word(word: &str) -> HlKind {
    if word == "true" || word == "false" {
        HlKind::Bool
    } else if TYPES.contains(&word) {
        HlKind::Type
    } else if KEYWORDS.contains(&word) {
        HlKind::Keyword
    } else {
        HlKind::Variable
    }
}

fn cmp_op(input: &mut LexIn) -> LexResult<()> {
    alt((
        literal("=="),
        literal("!="),
        literal("<="),
        literal(">="),
        literal("<"),
        literal(">"),
    ))
    .void()
    .parse_next(input)
}

fn punctuation(input: &mut LexIn) -> LexResult<()> {
    one_of([
        ':', ',', '(', ')', '[', ']', '.', ';', '=', '~', '+', '-', '*', '/', '{', '}', '!',
    ])
    .void()
    .parse_next(input)
}

#[cfg(test)]
mod tests;
