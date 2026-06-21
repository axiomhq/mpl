//! `chumsky` parser-combinator implementation of the `MPL` grammar.
//!
//! This module is the crate's parser. It replaces the former `pest` grammar
//! (`src/mpl.pest`) + tree-walk (`src/parser.rs`) and produces the existing AST
//! from [`crate::query`] directly from the combinators, carrying byte spans.
//!
//! Entry points:
//!
//! * [`parse_query`] — parse + resolve into a [`Query`] for [`crate::compile`],
//!   returning a single best-effort [`ParseError`] on failure (typed semantic
//!   errors win over generic syntax errors).
//! * [`parse`] — multi-error variant returning a best-effort AST plus *all*
//!   recovered [`ParseError`]s (used by the editor/diagnostics).
//! * [`parse_param_value`] — the external host-supplied-param entry point
//!   (the old `Rule::param_value`).
//! * [`highlight`] — a total (never-failing) lexer that classifies every byte
//!   range for syntax highlighting. Because it is lexical and total it survives
//!   arbitrary incomplete / mid-edit input, which drives the editor.
//!
//! ## Trivia / lossless CST
//!
//! `chumsky` is AST-oriented: the combinators throw trivia (whitespace and
//! `//` comments) away while building [`Query`]. The AST therefore is *not*
//! lossless. We recover the trivia at the **lexer** layer instead:
//! [`highlight`] emits a [`HlKind::Comment`] span for every comment, so the
//! token stream is lossless enough for highlighting. A formatter would need
//! that lexer extended to retain whitespace spans too (cheap) — see REPORT.md.
//!
//! ## Error model
//!
//! Syntax errors flow through `chumsky`'s [`Rich`] machinery; semantic errors
//! (unknown function, undefined param, …) are carried as a typed [`ParseError`]
//! inside the `Rich` *context* slot ([`Reason`]) so they survive merging and
//! recovery and surface verbatim. `chumsky`'s `Custom` reasons take priority
//! over `ExpectedFound` when two errors merge at the same position, which is
//! exactly what we want.

use std::{collections::HashMap, hash::BuildHasher};

use chrono::DateTime;
use chumsky::error::RichReason;
use chumsky::extra::SimpleState;
use chumsky::prelude::*;
use miette::SourceSpan;
use strumbra::SharedString;

use crate::{
    ParseError,
    enc_regex::EncodableRegex,
    linker::{AlignFunction, ComputeFunction, Function, FunctionId, GroupFunction, ModuleId},
    query::{
        Aggregate, Align, As, Cmp, DirectiveValue, Directives, Expr, Filter, FilterOrIfDef,
        MetricId, ParamDeclaration, ParamType, ParamValue, Params, ParseParamError, Query,
        RelativeTime, Source, StringFragment, TagExtend, TagType, TerminalParamType, Time,
        TimeRange, TimeUnit, WarningReason, Warnings,
    },
    stdlib::STDLIB,
    tags::TagValue,
    types::{BucketSpec, BucketType, ConversionMethod, Dataset, Metric, Parameterized},
};

#[cfg(test)]
mod tests;

// ───────────────────────────── highlighting ─────────────────────────────

/// Highlight classification for a byte range. Mirrors the editor's
/// `TokenType`; the language-server crate maps these 1:1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlKind {
    /// Identifiers, params (`$x`), tags, datasets, metrics.
    Variable,
    /// String literals.
    String,
    /// Numbers and relative times (`5m`).
    Number,
    /// `true` / `false`.
    Bool,
    /// Regex literals (`#/.../`, `#s/.../.../`).
    Regexp,
    /// Comparison / arithmetic operators.
    Operator,
    /// The `|` pipe.
    Punctuation,
    /// Reserved words.
    Keyword,
    /// Type names (`string`, `Duration`, `Option`, …).
    Type,
    /// `//` line comments.
    Comment,
}

/// A classified byte range, `[from, to)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Highlight {
    /// Start byte offset (inclusive).
    pub from: usize,
    /// End byte offset (exclusive).
    pub to: usize,
    /// The classification.
    pub kind: HlKind,
}

type HlExtra<'a> = extra::Err<Rich<'a, char>>;

pub(crate) fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}
pub(crate) fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Classifies a bare word for highlighting. Keyword/type classification is
/// lexical (set based) — the same precision the deleted `language.ts` regex
/// had, now in Rust. A tag that happens to be named like a keyword is the
/// only case where this differs from a position-aware walk.
fn classify_word(word: &str) -> HlKind {
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
    ];
    // Only the grammar's actual type literals — NOT lowercase `metric` /
    // `dataset` / `regex`, which are common identifier names.
    const TYPES: &[&str] = &[
        "string", "int", "float", "bool", "Dataset", "Duration", "duration", "Regex", "Option",
    ];

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

/// Tokenises `src` for syntax highlighting.
///
/// This is a **total** `chumsky` parser: a trailing `any()` catch-all means it
/// never errors, so it always returns spans — including for incomplete or
/// invalid input (an unterminated `"string`, a dangling `filter region == `).
/// That resilience is what lets the editor drop its JS regex fallback.
#[must_use]
pub fn highlight(src: &str) -> Vec<Highlight> {
    highlighter().parse(src).into_output().unwrap_or_default()
}

/// Builds a single-element highlight list from the current span. Returns a
/// `Vec` (not an `Option`) so every branch of [`highlighter`] — including the
/// `string` branch, which descends into `${ … }` and can emit *several*
/// highlights — unifies on one element type; the trailing catch-all returns an
/// empty `Vec`.
fn hl(span: SimpleSpan, kind: HlKind) -> Vec<Highlight> {
    vec![Highlight {
        from: span.start(),
        to: span.end(),
        kind,
    }]
}

/// One piece of a string literal during highlighting: a run of literal text
/// (carrying its byte span, so adjacent quotes can be merged into it) or a
/// `${ … }` interpolation already lowered to its inner highlights.
enum StrPart {
    Text(SimpleSpan),
    Interp(Vec<Highlight>),
}

/// Re-assembles a `"…${ … }…"` literal into highlights using the **same**
/// text/interpolation split the parser's [`string_expr`] uses: the literal
/// text fragments (with the surrounding quotes merged in) become [`HlKind::String`]
/// spans, the `${`/`}` delimiters and inner whitespace are left unclassified,
/// and each embedded expression keeps its own classification (a param/ident is
/// `Variable`, a number is `Number`, …). So `"host ${ $h } end"` yields
/// `String("\"host ")`, `Variable($h)`, `String(" end\"")`.
fn assemble_string(
    open: SimpleSpan,
    parts: Vec<StrPart>,
    close: Option<SimpleSpan>,
) -> Vec<Highlight> {
    let mut out = Vec::new();
    // The opening quote starts the first String run; text extends it, an
    // interpolation flushes it, and the closing quote (if any) extends the
    // trailing run.
    let mut cur: Option<(usize, usize)> = Some((open.start(), open.end()));
    let flush = |cur: &mut Option<(usize, usize)>, out: &mut Vec<Highlight>| {
        if let Some((from, to)) = cur.take() {
            out.push(Highlight {
                from,
                to,
                kind: HlKind::String,
            });
        }
    };
    for part in parts {
        match part {
            StrPart::Text(span) => {
                cur = Some(match cur {
                    Some((from, _)) => (from, span.end()),
                    None => (span.start(), span.end()),
                });
            }
            StrPart::Interp(mut hs) => {
                flush(&mut cur, &mut out);
                out.append(&mut hs);
            }
        }
    }
    if let Some(c) = close {
        cur = Some(match cur {
            Some((from, _)) => (from, c.end()),
            None => (c.start(), c.end()),
        });
    }
    flush(&mut cur, &mut out);
    out
}

fn highlighter<'a>() -> impl Parser<'a, &'a str, Vec<Highlight>, HlExtra<'a>> {
    // A single highlight *token* yielding zero, one, or (for an interpolated
    // string) several `Highlight`s. It is `recursive` so the `string` branch
    // can descend back into the same token set inside `${ … }` — including a
    // nested string — without a second grammar.
    let token = recursive::<_, Vec<Highlight>, HlExtra, _, _>(|token| {
        let esc = just::<_, _, HlExtra>('\\').then(any()).ignored();

        let comment = just("//")
            .then(none_of('\n').repeated())
            .map_with(|_, e| hl(e.span(), HlKind::Comment));

        // String literal with `${ … }` interpolation descent. Uses the same
        // text / interpolation split as the parser's `string_expr`: text runs
        // (escapes, `$` not followed by `{`, and any non-`"`/`\`/`$` char) stay
        // String; `${ … }` re-enters `token` so the embedded expression is
        // classified on its own.
        let str_char = choice((
            esc,
            just('$').then(just('{').not()).ignored(),
            none_of("\"\\$").ignored(),
        ));
        let text_part = str_char
            .repeated()
            .at_least(1)
            .to_slice()
            .map_with(|_: &str, e| StrPart::Text(e.span()));
        let interp_part = just("${")
            .ignore_then(
                token
                    .clone()
                    .and_is(just('}').not())
                    .repeated()
                    .collect::<Vec<Vec<Highlight>>>(),
            )
            .then_ignore(just('}').or_not())
            .map(|inner| StrPart::Interp(inner.into_iter().flatten().collect()));
        let string = just('"')
            .map_with(|_, e| e.span())
            .then(
                choice((text_part, interp_part))
                    .repeated()
                    .collect::<Vec<_>>(),
            )
            .then(just('"').map_with(|_, e| e.span()).or_not())
            .map(|((open, parts), close)| assemble_string(open, parts, close));

        let regex_body = choice((esc, none_of("/\\").ignored())).repeated();
        let regex_replace = just("#s/")
            .then(regex_body)
            .then(just('/'))
            .then(regex_body)
            .then(just('/').or_not())
            .map_with(|_, e| hl(e.span(), HlKind::Regexp));
        let regex = just("#/")
            .then(regex_body)
            .then(just('/').or_not())
            .map_with(|_, e| hl(e.span(), HlKind::Regexp));

        let escaped_ident = just('`')
            .then(choice((esc, none_of("`\\").ignored())).repeated())
            .then(just('`').or_not())
            .map_with(|_, e| hl(e.span(), HlKind::Variable));

        let param = just('$')
            .then(any().filter(|c: &char| is_ident_continue(*c)).repeated())
            .map_with(|_, e| hl(e.span(), HlKind::Variable));

        let digit = any().filter(|c: &char| c.is_ascii_digit());
        let frac = just('.').then(digit.repeated());
        let exp = one_of("eE")
            .then(one_of("+-").or_not())
            .then(digit.repeated());
        let unit = choice((just("ms").ignored(), one_of("smhdwMy").ignored()));
        let number = digit
            .repeated()
            .at_least(1)
            .then(frac.or_not())
            .then(exp.or_not())
            .then(unit.or_not())
            .map_with(|_, e| hl(e.span(), HlKind::Number));

        let word = any()
            .filter(|c: &char| is_ident_start(*c))
            .then(any().filter(|c: &char| is_ident_continue(*c)).repeated())
            .to_slice()
            .map_with(|w: &str, e| hl(e.span(), classify_word(w)));

        let multi_op = choice((just("=="), just("!="), just("<="), just(">=")))
            .map_with(|_, e| hl(e.span(), HlKind::Operator));
        let single_op = one_of("<>+-*/").map_with(|_, e| hl(e.span(), HlKind::Operator));
        let pipe = just('|').map_with(|_, e| hl(e.span(), HlKind::Punctuation));

        let other = any().to(Vec::new());

        choice((
            comment,
            regex_replace,
            regex,
            string,
            escaped_ident,
            param,
            number,
            word,
            multi_op,
            single_op,
            pipe,
            other,
        ))
        .boxed()
    });

    token
        .repeated()
        .collect::<Vec<_>>()
        .map(|v| v.into_iter().flatten().collect())
}

// ───────────────────────────── error carrier ─────────────────────────────

/// The `Rich` *context* slot: either a generic message or a fully-formed typed
/// [`ParseError`] payload (semantic errors) we want to surface verbatim.
#[derive(Debug, Clone)]
pub(crate) enum Reason {
    /// A plain message; mapped to [`ParseError::SyntaxError`].
    Msg(String),
    /// A typed semantic error payload.
    Sem(Box<Sem>),
}

impl std::fmt::Display for Reason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reason::Msg(m) => f.write_str(m),
            Reason::Sem(s) => write!(f, "{s:?}"),
        }
    }
}

impl From<String> for Reason {
    fn from(s: String) -> Self {
        Reason::Msg(s)
    }
}
impl From<&str> for Reason {
    fn from(s: &str) -> Self {
        Reason::Msg(s.to_string())
    }
}

/// Typed semantic errors raised inside combinators. Cloneable (so `chumsky`'s
/// error machinery can move/merge them) and convertible to the crate's public
/// [`ParseError`].
#[derive(Debug, Clone)]
pub(crate) enum Sem {
    Syntax {
        span: SourceSpan,
        label: String,
        message: String,
    },
    UndefinedParam {
        span: SourceSpan,
        param: String,
    },
    ParamDefinedMultipleTimes {
        span: SourceSpan,
        param: String,
    },
    UnsupportedAlignFunction {
        span: SourceSpan,
        name: String,
    },
    UnsupportedMapFunction {
        span: SourceSpan,
        name: String,
    },
    UnsupportedMapEvaluation {
        span: SourceSpan,
        name: String,
    },
    UnsupportedGroupFunction {
        span: SourceSpan,
        name: String,
    },
    UnsupportedComputeFunction {
        span: SourceSpan,
        name: String,
    },
    IfdefNotOptional {
        span: SourceSpan,
        param: ParamDeclaration,
    },
    NotSupported {
        span: SourceSpan,
        feature: String,
    },
    NotImplemented(&'static str),
    InvalidRegex {
        span: SourceSpan,
        message: String,
    },
    Strumbra {
        span: SourceSpan,
        message: String,
    },
}

impl Sem {
    fn into_parse_error(self) -> ParseError {
        match self {
            Sem::Syntax {
                span,
                label,
                message,
            } => ParseError::SyntaxError {
                span,
                label,
                message,
                suggestion: None,
            },
            Sem::UndefinedParam { span, param } => ParseError::UndefinedParam { span, param },
            Sem::ParamDefinedMultipleTimes { span, param } => {
                ParseError::ParamDefinedMultipleTimes { span, param }
            }
            Sem::UnsupportedAlignFunction { span, name } => {
                ParseError::UnsupportedAlignFunction { span, name }
            }
            Sem::UnsupportedMapFunction { span, name } => {
                ParseError::UnsupportedMapFunction { span, name }
            }
            Sem::UnsupportedMapEvaluation { span, name } => {
                ParseError::UnsupportedMapEvaluation { span, name }
            }
            Sem::UnsupportedGroupFunction { span, name } => {
                ParseError::UnsupportedGroupFunction { span, name }
            }
            Sem::UnsupportedComputeFunction { span, name } => {
                ParseError::UnsupportedComputeFunction { span, name }
            }
            Sem::IfdefNotOptional { span, param } => ParseError::IfdefNotOptional { span, param },
            Sem::NotSupported { span, feature } => ParseError::NotSupported {
                span,
                rule: feature,
            },
            Sem::NotImplemented(what) => ParseError::NotImplemented(what),
            Sem::InvalidRegex { span, message } | Sem::Strumbra { span, message } => {
                ParseError::SyntaxError {
                    span,
                    label: message.clone(),
                    message,
                    suggestion: None,
                }
            }
        }
    }
}

type Err<'a> = Rich<'a, char, SimpleSpan, Reason>;

fn rich<'a>(span: SourceSpan, sem: Sem) -> Err<'a> {
    let start = span.offset();
    let s: SimpleSpan = (start..start + span.len()).into();
    Rich::custom(s, Reason::Sem(Box::new(sem)))
}

/// Maps a recovered `chumsky` error into the crate's public [`ParseError`].
fn to_parse_error(err: &Err<'_>) -> ParseError {
    match err.reason() {
        RichReason::Custom(Reason::Sem(sem)) => (**sem).clone().into_parse_error(),
        RichReason::Custom(Reason::Msg(msg)) => ParseError::SyntaxError {
            span: rich_span(err),
            label: msg.clone(),
            message: msg.clone(),
            suggestion: None,
        },
        RichReason::ExpectedFound { .. } => ParseError::SyntaxError {
            span: rich_span(err),
            label: expected_label(err),
            message: err.to_string(),
            suggestion: None,
        },
    }
}

/// Formats an `ExpectedFound` error's expected set as `expected one of:\n- …`,
/// the shape downstream diagnostics (editor + playground) already parse.
fn expected_label(err: &Err<'_>) -> String {
    let expected: Vec<String> = err.expected().map(ToString::to_string).collect();
    if expected.is_empty() {
        return "unexpected token".to_string();
    }
    let mut label = String::from("expected one of:");
    for e in &expected {
        label.push_str("\n- ");
        label.push_str(e);
    }
    label
}

fn rich_span(err: &Err<'_>) -> SourceSpan {
    let span = err.span();
    let start = span.start();
    SourceSpan::new(start.into(), span.end().saturating_sub(start))
}

// ───────────────────────────── AST parsing ─────────────────────────────

/// Parser state threaded through `chumsky`: declared params (so `$x`
/// references resolve to their [`ParamDeclaration`] during the same parse),
/// collected directives, and non-fatal warnings.
#[derive(Default)]
struct SliceState {
    params: Params,
    directives: Directives,
    warnings: Warnings,
}

type St = SimpleState<SliceState>;
type Extra<'a> = extra::Full<Err<'a>, St, ()>;

/// The result of [`parse`]: a best-effort AST plus any recovered errors and
/// warnings. `query` is `None` only when even the source could not be parsed.
pub struct SliceParse {
    /// The recovered query AST, if one could be built.
    pub query: Option<Query>,
    /// Parse errors mapped into the crate's miette shape.
    pub errors: Vec<ParseError>,
    /// Non-fatal warnings (e.g. the deprecated lowercase `duration`).
    pub warnings: Warnings,
}

/// Parses `src` into a [`Query`] using `chumsky` with error recovery,
/// collecting *all* recovered errors (multi-error).
#[must_use]
pub fn parse(src: &str) -> SliceParse {
    let mut state: St = SimpleState(SliceState::default());
    let result = file().parse_with_state(src, &mut state);
    let query = result.output().cloned();
    let errors = result.errors().map(to_parse_error).collect();
    SliceParse {
        query,
        errors,
        warnings: state.0.warnings,
    }
}

/// Parses + resolves `src` into a [`Query`] for [`crate::compile`].
///
/// `system_params` seeds host-supplied params (e.g. `$__interval`) so they
/// resolve during the parse. Returns a single best-effort error on failure,
/// preferring a typed semantic error over a generic syntax error.
pub fn parse_query<S: BuildHasher>(
    src: &str,
    system_params: HashMap<String, ParamType, S>,
) -> Result<(Query, Warnings), ParseError> {
    let mut state = SliceState::default();
    for (name, typ) in system_params {
        if !name.starts_with("__") {
            return Err(ParseError::SystemParamMissingPrefix { param: name });
        }
        state.params.push(ParamDeclaration {
            span: SourceSpan::new(0.into(), 0),
            name,
            typ,
        });
    }

    let mut st: St = SimpleState(state);
    let result = file().parse_with_state(src, &mut st);
    let query = result.output().cloned();
    let mut errors: Vec<ParseError> = result.errors().map(to_parse_error).collect();

    if !errors.is_empty() {
        // Prefer a typed semantic error (function lookup, undefined param, …)
        // over a generic syntax error so editor quick-fixes work.
        let idx = errors
            .iter()
            .position(|e| !matches!(e, ParseError::SyntaxError { .. }))
            .unwrap_or(0);
        return Err(errors.swap_remove(idx));
    }

    let query = query.ok_or(ParseError::EOF {
        span: SourceSpan::new(0.into(), 0),
    })?;
    Ok((query, st.0.warnings))
}

/// What a single `| …` clause contributes to the query.
#[derive(Clone)]
enum Clause {
    Filter(FilterOrIfDef),
    Agg(Aggregate),
    Extend(Vec<TagExtend>),
    Sample(f64),
    /// A clause that failed to parse and was recovered/skipped.
    Skipped,
}

/// Comparison operator kind for value filters.
#[derive(Clone, Copy)]
enum CmpKind {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
}

/// Regex comparison operator kind (`==` / `!=` only).
#[derive(Clone, Copy)]
enum ReOp {
    Eq,
    Ne,
}

fn to_span(span: SimpleSpan) -> SourceSpan {
    SourceSpan::new(span.start().into(), span.end().saturating_sub(span.start()))
}

fn placeholder(span: SourceSpan, name: String) -> ParamDeclaration {
    ParamDeclaration {
        span,
        name,
        typ: ParamType::Terminal(TerminalParamType::Tag(TagType::String)),
    }
}

/// Consumes trailing trivia (whitespace + `//` comments) after `p`.
fn lex<'a, O: 'a>(
    p: impl Parser<'a, &'a str, O, Extra<'a>> + Clone,
) -> impl Parser<'a, &'a str, O, Extra<'a>> + Clone {
    p.then_ignore(trivia())
}

fn trivia<'a>() -> impl Parser<'a, &'a str, (), Extra<'a>> + Clone {
    let comment = just("//").then(none_of('\n').repeated()).ignored();
    let ws = any().filter(|c: &char| c.is_whitespace()).ignored();
    choice((ws, comment)).repeated().ignored()
}

/// A reserved word with a trailing word boundary, plus trailing trivia.
fn kw<'a>(word: &'static str) -> impl Parser<'a, &'a str, (), Extra<'a>> + Clone {
    lex(text::ascii::keyword::<&'a str, &'static str, Extra<'a>>(word).ignored())
}

fn sym<'a>(s: &'static str) -> impl Parser<'a, &'a str, (), Extra<'a>> + Clone {
    lex(just(s).ignored())
}

fn plain_ident_raw<'a>() -> impl Parser<'a, &'a str, String, Extra<'a>> + Clone {
    any()
        .filter(|c: &char| is_ident_start(*c))
        .then(any().filter(|c: &char| is_ident_continue(*c)).repeated())
        .to_slice()
        .map(ToString::to_string)
}

fn escaped_ident_raw<'a>() -> impl Parser<'a, &'a str, String, Extra<'a>> + Clone {
    let esc = just('\\').then(any()).ignored();
    just('`')
        .then(choice((esc, none_of("`\\").ignored())).repeated())
        .then(just('`'))
        .to_slice()
        .map(|s: &str| unescape(&s[1..s.len() - 1], '`'))
}

/// `plain_ident | escaped_ident`, unescaped, with trailing trivia.
fn ident<'a>() -> impl Parser<'a, &'a str, String, Extra<'a>> + Clone {
    lex(choice((escaped_ident_raw(), plain_ident_raw())))
}

/// `$name` resolved against declared params. Returns the use-site span plus
/// the resolved declaration. Undefined params emit an error and fall back to a
/// string-typed placeholder so recovery can continue.
fn resolved_param<'a>()
-> impl Parser<'a, &'a str, (SourceSpan, ParamDeclaration), Extra<'a>> + Clone {
    lex(just('$')
        .ignore_then(plain_ident_raw())
        .map_with(|name, e| (name, to_span(e.span()))))
    .validate(|(name, span), e, emitter| {
        if let Some(param) = e.state().params.iter().find(|p| p.name == name).cloned() {
            (span, param)
        } else {
            emitter.emit(rich(
                span,
                Sem::UndefinedParam {
                    span,
                    param: name.clone(),
                },
            ));
            (span, placeholder(span, name))
        }
    })
}

/// A string literal as an [`Expr`], including `${ … }` interpolation. The
/// body recursively re-enters [`expr`]. An all-text literal collapses into a
/// single `Const(String)`, matching the formatter.
fn string_expr<'a>(
    expr: impl Parser<'a, &'a str, Expr, Extra<'a>> + Clone + 'a,
) -> impl Parser<'a, &'a str, Expr, Extra<'a>> + Clone {
    let escape = just('\\').then(any()).ignored();
    // `$` is text unless it begins a `${` interpolation.
    let dollar = just('$').then_ignore(just('{').not());
    let plain = none_of("\"\\$").ignored();
    let text = choice((escape, dollar.ignored(), plain))
        .repeated()
        .at_least(1)
        .to_slice()
        .map(|s: &str| StringFragment::Text(unescape(s, '"')));
    let interp = just("${")
        .ignore_then(trivia())
        .ignore_then(expr)
        .then_ignore(just('}'))
        .map(StringFragment::Expr);
    lex(just('"')
        .ignore_then(choice((text, interp)).repeated().collect::<Vec<_>>())
        .then_ignore(just('"'))
        .try_map(|frags, span| collapse_fragments(frags, to_span(span))))
}

fn collapse_fragments(frags: Vec<StringFragment>, span: SourceSpan) -> Result<Expr, Err<'static>> {
    if frags.iter().all(|f| matches!(f, StringFragment::Text(_))) {
        let s: String = frags
            .into_iter()
            .map(|f| match f {
                StringFragment::Text(t) => t,
                StringFragment::Expr(_) => String::new(),
            })
            .collect();
        SharedString::try_from(s)
            .map(|s| Expr::Const(TagValue::String(s)))
            .map_err(|e| {
                rich(
                    span,
                    Sem::Strumbra {
                        span,
                        message: e.to_string(),
                    },
                )
            })
    } else {
        Ok(Expr::String(frags))
    }
}

/// A plain (non-interpolated) string, unescaped — used by directives and
/// host-supplied param values, matching the old `pest` behavior of taking the
/// literal text.
fn plain_string<'a>() -> impl Parser<'a, &'a str, String, Extra<'a>> + Clone {
    let esc = just('\\').then(any()).ignored();
    lex(just('"')
        .ignore_then(
            choice((esc, none_of("\"\\").ignored()))
                .repeated()
                .to_slice(),
        )
        .then_ignore(just('"'))
        .map(|s: &str| unescape(s, '"')))
}

/// An int or float literal, including `inf` / `+inf` / `-inf`.
fn number_const<'a>() -> impl Parser<'a, &'a str, TagValue, Extra<'a>> + Clone {
    let word_end = any().filter(|c: &char| is_ident_continue(*c)).not();
    let inf = lex(choice((
        just("-inf").to(f64::NEG_INFINITY),
        just("+inf").to(f64::INFINITY),
        just("inf").to(f64::INFINITY),
    ))
    .then_ignore(word_end)
    .map(TagValue::Float));

    let digit = any().filter(|c: &char| c.is_ascii_digit());
    let frac = just('.').then(digit.repeated());
    let exp = one_of("eE")
        .then(one_of("+-").or_not())
        .then(digit.repeated());
    let num = lex(one_of("+-")
        .or_not()
        .then(digit.repeated().at_least(1))
        .then(frac.or_not())
        .then(exp.or_not())
        .to_slice()
        .try_map(|s: &str, span| {
            if s.contains('.') || s.contains('e') || s.contains('E') {
                s.parse::<f64>().map(TagValue::Float).map_err(|e| {
                    rich(
                        to_span(span),
                        Sem::Syntax {
                            span: to_span(span),
                            label: e.to_string(),
                            message: e.to_string(),
                        },
                    )
                })
            } else {
                s.parse::<i64>().map(TagValue::Int).map_err(|e| {
                    rich(
                        to_span(span),
                        Sem::Syntax {
                            span: to_span(span),
                            label: e.to_string(),
                            message: e.to_string(),
                        },
                    )
                })
            }
        }));

    choice((inf, num))
}

#[allow(clippy::cast_precision_loss)]
fn number_f64<'a>() -> impl Parser<'a, &'a str, f64, Extra<'a>> + Clone {
    number_const().map(|tv| match tv {
        TagValue::Int(i) => i as f64,
        TagValue::Float(f) => f,
        _ => f64::NAN,
    })
}

fn bool_lit<'a>() -> impl Parser<'a, &'a str, bool, Extra<'a>> + Clone {
    choice((kw("true").to(true), kw("false").to(false)))
}

fn regex_lit<'a>() -> impl Parser<'a, &'a str, EncodableRegex, Extra<'a>> + Clone {
    let esc = just('\\').then(any()).ignored();
    lex(just("#/")
        .ignore_then(
            choice((esc, none_of("/\\").ignored()))
                .repeated()
                .to_slice(),
        )
        .then_ignore(just('/'))
        .try_map(|inner: &str, span| {
            EncodableRegex::new(inner).map_err(|e| {
                rich(
                    to_span(span),
                    Sem::InvalidRegex {
                        span: to_span(span),
                        message: e.to_string(),
                    },
                )
            })
        }))
}

/// `#s/src/dst/` — parsed (for `replace`) but the value is discarded since the
/// runtime does not support replace yet.
fn regex_replace_lit<'a>() -> impl Parser<'a, &'a str, (), Extra<'a>> + Clone {
    let esc = just('\\').then(any()).ignored();
    let body = choice((esc, none_of("/\\").ignored())).repeated();
    lex(just("#s/")
        .then(body)
        .then(just('/'))
        .then(body)
        .then(just('/'))
        .ignored())
}

fn time_unit<'a>() -> impl Parser<'a, &'a str, TimeUnit, Extra<'a>> + Clone {
    choice((
        just("ms").to(TimeUnit::Millisecond),
        just('s').to(TimeUnit::Second),
        just('m').to(TimeUnit::Minute),
        just('h').to(TimeUnit::Hour),
        just('d').to(TimeUnit::Day),
        just('w').to(TimeUnit::Week),
        just('M').to(TimeUnit::Month),
        just('y').to(TimeUnit::Year),
    ))
}

fn digits<'a>() -> impl Parser<'a, &'a str, &'a str, Extra<'a>> + Clone {
    any()
        .filter(|c: &char| c.is_ascii_digit())
        .repeated()
        .at_least(1)
        .to_slice()
}

fn relative_time<'a>() -> impl Parser<'a, &'a str, RelativeTime, Extra<'a>> + Clone {
    lex(digits().then(time_unit()).try_map(|(d, unit), span| {
        d.parse::<u64>()
            .map(|value| RelativeTime { value, unit })
            .map_err(|e| {
                rich(
                    to_span(span),
                    Sem::Syntax {
                        span: to_span(span),
                        label: e.to_string(),
                        message: e.to_string(),
                    },
                )
            })
    }))
}

/// `[ <time> .. <time>? ]` with all `pest` time variants: relative, RFC3339,
/// absolute timestamp, and `±<reltime>` modifier.
fn time<'a>() -> impl Parser<'a, &'a str, Time, Extra<'a>> + Clone {
    let digit = any().filter(|c: &char| c.is_ascii_digit());

    let relative = digits()
        .then(time_unit())
        .try_map(|(d, unit), span| {
            d.parse::<u64>()
                .map(|value| Time::Relative(RelativeTime { value, unit }))
                .map_err(|e| rich(to_span(span), syntax(span, &e.to_string())))
        })
        .boxed();

    // 2025-03-01T13:00:00Z
    let rfc = digit
        .repeated()
        .exactly(4)
        .then(just('-'))
        .then(digit.repeated().exactly(2))
        .then(just('-'))
        .then(digit.repeated().exactly(2))
        .then(just('T'))
        .then(digit.repeated().exactly(2))
        .then(just(':'))
        .then(digit.repeated().exactly(2))
        .then(just(':'))
        .then(digit.repeated().exactly(2))
        .then(just('Z').or_not())
        .to_slice()
        .try_map(|s: &str, span| {
            DateTime::parse_from_rfc3339(s)
                .map(Time::RFC3339)
                .map_err(|e| rich(to_span(span), syntax(span, &e.to_string())))
        })
        .boxed();

    let timestamp = digits()
        .try_map(|s: &str, span| {
            s.parse::<i64>()
                .map(Time::Timestamp)
                .map_err(|e| rich(to_span(span), syntax(span, &e.to_string())))
        })
        .boxed();

    let modifier = one_of("+-")
        .then(digits())
        .then(time_unit())
        .to_slice()
        .map(|s: &str| Time::Modifier(s.to_string()))
        .boxed();

    lex(choice((relative, rfc, timestamp, modifier)))
}

fn syntax(span: SimpleSpan, message: &str) -> Sem {
    Sem::Syntax {
        span: to_span(span),
        label: message.to_string(),
        message: message.to_string(),
    }
}

fn time_range<'a>() -> impl Parser<'a, &'a str, TimeRange, Extra<'a>> + Clone {
    sym("[")
        .ignore_then(time())
        .then_ignore(sym(".."))
        .then(time().or_not())
        .then_ignore(sym("]"))
        .map(|(start, end)| TimeRange { start, end })
}

/// `<reltime> | $param` — used by `align to/over` and `bucket to`.
fn parameterized_relative_time<'a>()
-> impl Parser<'a, &'a str, Parameterized<RelativeTime>, Extra<'a>> + Clone {
    choice((
        resolved_param().map(|(span, param)| Parameterized::Param { span, param }),
        relative_time().map(Parameterized::Concrete),
    ))
}

fn metric_name<'a>() -> impl Parser<'a, &'a str, Metric, Extra<'a>> + Clone {
    ident().try_map(|s, span| {
        Metric::try_from(s).map_err(|e| {
            rich(
                to_span(span),
                Sem::Strumbra {
                    span: to_span(span),
                    message: e.to_string(),
                },
            )
        })
    })
}

/// Dataset token (`ident | $param`) plus its raw byte span (excluding trailing
/// trivia), used so a missing `:` can pin its error on the dataset itself the
/// way the old `pest` path did.
fn dataset_spanned<'a>()
-> impl Parser<'a, &'a str, (Parameterized<Dataset>, SourceSpan), Extra<'a>> + Clone {
    let param = just('$')
        .ignore_then(plain_ident_raw())
        .map_with(|name, e| (name, to_span(e.span())))
        .validate(|(name, span), e, emitter| {
            let dataset =
                if let Some(param) = e.state().params.iter().find(|p| p.name == name).cloned() {
                    Parameterized::Param { span, param }
                } else {
                    emitter.emit(rich(
                        span,
                        Sem::UndefinedParam {
                            span,
                            param: name.clone(),
                        },
                    ));
                    Parameterized::Param {
                        span,
                        param: placeholder(span, name),
                    }
                };
            (dataset, span)
        });
    let plain = choice((escaped_ident_raw(), plain_ident_raw()))
        .map_with(|s, e| (Parameterized::Concrete(Dataset::new(s)), to_span(e.span())));
    lex(choice((param, plain)))
}

/// `dataset:metric`. A missing `:` errors over the dataset; a missing metric
/// name errors at the offending token after the colon.
fn metric_id<'a>() -> impl Parser<'a, &'a str, MetricId, Extra<'a>> + Clone {
    #[derive(Clone)]
    enum Tail {
        Metric(Metric),
        MissingMetric(SourceSpan),
        NoColon,
    }

    let colon_tail = sym(":").ignore_then(choice((
        metric_name().map(Tail::Metric),
        any().map_with(|_, e| Tail::MissingMetric(to_span(e.span()))),
        empty().map_with(|(), e| Tail::MissingMetric(to_span(e.span()))),
    )));

    dataset_spanned()
        .then(choice((colon_tail, empty().to(Tail::NoColon))))
        .try_map(|((dataset, dspan), tail), _| match tail {
            Tail::Metric(metric) => Ok(MetricId { dataset, metric }),
            Tail::MissingMetric(s) => Err(rich(
                s,
                Sem::Syntax {
                    span: s,
                    label: "expected metric name".to_string(),
                    message: "expected a metric name".to_string(),
                },
            )),
            Tail::NoColon => Err(rich(
                dspan,
                Sem::Syntax {
                    span: dspan,
                    label: "expected ':' and a metric name".to_string(),
                    message: "expected a metric identifier (e.g. dataset:metric)".to_string(),
                },
            )),
        })
}

fn as_clause<'a>() -> impl Parser<'a, &'a str, As, Extra<'a>> + Clone {
    kw("as").ignore_then(metric_name()).map(|name| As { name })
}

/// `source = metric_id time_range? as?` → `(Source, Option<As>)`.
fn source<'a>() -> impl Parser<'a, &'a str, (Source, Option<As>), Extra<'a>> + Clone {
    metric_id()
        .then(time_range().or_not())
        .then(as_clause().or_not())
        .map(|((metric_id, time), as_)| (Source { metric_id, time }, as_))
}

fn expr<'a>() -> impl Parser<'a, &'a str, Expr, Extra<'a>> + Clone {
    recursive(|expr| {
        let string = string_expr(expr);
        choice((
            resolved_param().map(|(span, param)| Expr::Param { span, param }),
            string,
            number_const().map(Expr::Const),
            ident().map(|s| match s.as_str() {
                "true" => Expr::Const(TagValue::Bool(true)),
                "false" => Expr::Const(TagValue::Bool(false)),
                _ => Expr::Tag(s),
            }),
        ))
    })
}

fn cmp<'a>() -> impl Parser<'a, &'a str, CmpKind, Extra<'a>> + Clone {
    choice((
        sym("==").to(CmpKind::Eq),
        sym("!=").to(CmpKind::Ne),
        sym("<=").to(CmpKind::Le),
        sym(">=").to(CmpKind::Ge),
        sym("<").to(CmpKind::Lt),
        sym(">").to(CmpKind::Gt),
    ))
}

fn cmp_re<'a>() -> impl Parser<'a, &'a str, ReOp, Extra<'a>> + Clone {
    choice((sym("==").to(ReOp::Eq), sym("!=").to(ReOp::Ne)))
}

fn tag_type<'a>() -> impl Parser<'a, &'a str, TagType, Extra<'a>> + Clone {
    choice((
        kw("string").to(TagType::String),
        kw("int").to(TagType::Int),
        kw("float").to(TagType::Float),
        kw("bool").to(TagType::Bool),
    ))
}

fn value_cmp(kind: CmpKind, e: Expr) -> Cmp {
    match kind {
        CmpKind::Eq => Cmp::Eq(e),
        CmpKind::Ne => Cmp::Ne(e),
        CmpKind::Gt => Cmp::Gt(e),
        CmpKind::Ge => Cmp::Ge(e),
        CmpKind::Lt => Cmp::Lt(e),
        CmpKind::Le => Cmp::Le(e),
    }
}

/// `tag (cmp expr | cmp_re regex | is tag_type)`.
///
/// The `== #/regex/` vs `== $param` ambiguity is handled by **ordered choice**:
/// the regex branch is tried first; `== #/x/` matches it, while `== $p` makes
/// the regex literal fail so `chumsky` rewinds and takes the value branch,
/// producing `Cmp::Eq(Param)`. Whether that param is a regex is decided later
/// by the typecheck pass, exactly as before.
fn filter_atom<'a>() -> impl Parser<'a, &'a str, Filter, Extra<'a>> + Clone {
    let rhs = choice((
        kw("is").ignore_then(tag_type()).map(Cmp::Is),
        cmp_re().then(regex_lit()).map(|(op, re)| match op {
            ReOp::Eq => Cmp::RegEx(Parameterized::Concrete(re)),
            ReOp::Ne => Cmp::RegExNot(Parameterized::Concrete(re)),
        }),
        cmp().then(expr()).map(|(op, e)| value_cmp(op, e)),
    ));
    ident()
        .then(rhs)
        .map(|(field, rhs)| Filter::Cmp { field, rhs })
}

fn collapse(mut v: Vec<Filter>, combine: fn(Vec<Filter>) -> Filter) -> Filter {
    if v.len() == 1 {
        v.remove(0)
    } else {
        combine(v)
    }
}

/// `filter_or` with `and`/`or`/`not` and parenthesised groups (recovered via
/// `nested_delimiters`).
fn filter<'a>() -> impl Parser<'a, &'a str, Filter, Extra<'a>> + Clone {
    recursive(|or| {
        let clause = choice((
            filter_atom(),
            or.clone()
                .delimited_by(sym("("), sym(")"))
                .recover_with(via_parser(nested_delimiters(
                    '(',
                    ')',
                    [('[', ']')],
                    |_| Filter::And(Vec::new()),
                ))),
        ));
        let not = choice((
            kw("not")
                .ignore_then(clause.clone())
                .map(|f| Filter::Not(Box::new(f))),
            clause,
        ));
        let and = not
            .separated_by(kw("and"))
            .at_least(1)
            .collect::<Vec<_>>()
            .map(|v| collapse(v, Filter::And));
        and.separated_by(kw("or"))
            .at_least(1)
            .collect::<Vec<_>>()
            .map(|v| collapse(v, Filter::Or))
    })
}

/// `filter <filter_or>` / `where <filter_or>`.
fn filter_expr<'a>() -> impl Parser<'a, &'a str, Filter, Extra<'a>> + Clone {
    choice((kw("filter"), kw("where"))).ignore_then(filter())
}

/// `(module ::)* name` resolved against the relevant stdlib category.
fn func_id<'a>() -> impl Parser<'a, &'a str, (Function, SourceSpan), Extra<'a>> + Clone {
    let module = plain_ident_raw().then_ignore(just("::"));
    lex(module
        .repeated()
        .collect::<Vec<String>>()
        .then(plain_ident_raw())
        .map_with(|(modules, name), e| {
            (
                Function {
                    module_path: modules.iter().map(|m| ModuleId::new(m)).collect(),
                    name: FunctionId::new(&name),
                },
                to_span(e.span()),
            )
        }))
}

/// A function name OR a bare operator (`+ - * /`), for `map`/`compute`.
fn func_or_op<'a>() -> impl Parser<'a, &'a str, (Function, SourceSpan), Extra<'a>> + Clone {
    choice((
        func_id(),
        lex(one_of("+-*/").to_slice().map_with(|s: &str, e| {
            (
                Function {
                    module_path: vec![],
                    name: FunctionId::new(s),
                },
                to_span(e.span()),
            )
        })),
    ))
}

fn align_func<'a>() -> impl Parser<'a, &'a str, AlignFunction, Extra<'a>> + Clone {
    func_id().try_map(|(func, span), _| {
        STDLIB.align_fn(&func).cloned().ok_or_else(|| {
            rich(
                span,
                Sem::UnsupportedAlignFunction {
                    span,
                    name: func.to_string(),
                },
            )
        })
    })
}

fn group_func<'a>() -> impl Parser<'a, &'a str, GroupFunction, Extra<'a>> + Clone {
    func_id().try_map(|(func, span), _| {
        STDLIB.group_fn(&func).cloned().ok_or_else(|| {
            rich(
                span,
                Sem::UnsupportedGroupFunction {
                    span,
                    name: func.to_string(),
                },
            )
        })
    })
}

fn compute_func<'a>() -> impl Parser<'a, &'a str, ComputeFunction, Extra<'a>> + Clone {
    func_or_op().try_map(|(func, span), _| {
        STDLIB.compute_fn(&func).cloned().ok_or_else(|| {
            rich(
                span,
                Sem::UnsupportedComputeFunction {
                    span,
                    name: func.to_string(),
                },
            )
        })
    })
}

/// `align (to <time>)? (over <time>)? using <func>`. `over` (sliding window) is
/// parsed but unsupported, mirroring the old `NotImplemented("sliding windows")`.
fn align<'a>() -> impl Parser<'a, &'a str, Align, Extra<'a>> + Clone {
    kw("align")
        .ignore_then(kw("to").ignore_then(parameterized_relative_time()).or_not())
        .then(
            kw("over")
                .ignore_then(parameterized_relative_time())
                .or_not(),
        )
        .then(kw("using").ignore_then(align_func()))
        .validate(|((time, over), function), e, emitter| {
            if over.is_some() {
                emitter.emit(rich(
                    to_span(e.span()),
                    Sem::NotImplemented("sliding windows"),
                ));
            }
            Align { function, time }
        })
}

/// `map (<op> <number> | <func> ("(" <number> ")")?)`.
fn map_clause<'a>() -> impl Parser<'a, &'a str, crate::query::Mapping, Extra<'a>> + Clone {
    let eval = lex(one_of("+-*/").to_slice().map(ToString::to_string))
        .then(number_f64())
        .try_map(|(op, arg), span| {
            let func = Function {
                module_path: vec![],
                name: FunctionId::new(&op),
            };
            STDLIB
                .map_fn(&func)
                .cloned()
                .map(|function| crate::query::Mapping {
                    function,
                    arg: Some(arg),
                })
                .ok_or_else(|| {
                    rich(
                        to_span(span),
                        Sem::UnsupportedMapEvaluation {
                            span: to_span(span),
                            name: op,
                        },
                    )
                })
        });
    let func = func_id()
        .then(
            sym("(")
                .ignore_then(number_f64())
                .then_ignore(sym(")"))
                .or_not(),
        )
        .try_map(|((func, span), arg), _| {
            STDLIB
                .map_fn(&func)
                .cloned()
                .map(|function| crate::query::Mapping { function, arg })
                .ok_or_else(|| {
                    rich(
                        span,
                        Sem::UnsupportedMapFunction {
                            span,
                            name: func.to_string(),
                        },
                    )
                })
        });
    kw("map").ignore_then(choice((eval, func)))
}

fn tags<'a>() -> impl Parser<'a, &'a str, Vec<String>, Extra<'a>> + Clone {
    ident().separated_by(sym(",")).at_least(1).collect()
}

/// `group ("by" <tags>)? "using" <func>`.
fn group_by<'a>() -> impl Parser<'a, &'a str, crate::query::GroupBy, Extra<'a>> + Clone {
    kw("group")
        .ignore_then(kw("by").ignore_then(tags()).or_not())
        .then_ignore(kw("using"))
        .then(group_func())
        .map_with(|(tags, function), e| crate::query::GroupBy {
            span: to_span(e.span()),
            function,
            tags: tags.unwrap_or_default(),
        })
}

fn bucket_conversion<'a>() -> impl Parser<'a, &'a str, ConversionMethod, Extra<'a>> + Clone {
    choice((
        kw("rate").to(ConversionMethod::Rate),
        kw("increase").to(ConversionMethod::Increase),
    ))
}

fn bucket_spec<'a>() -> impl Parser<'a, &'a str, BucketSpec, Extra<'a>> + Clone {
    choice((
        kw("count").to(BucketSpec::Count),
        kw("avg").to(BucketSpec::Avg),
        kw("sum").to(BucketSpec::Sum),
        kw("min").to(BucketSpec::Min),
        kw("max").to(BucketSpec::Max),
        number_f64().map(BucketSpec::Percentile),
    ))
}

fn bucket_specs<'a>() -> impl Parser<'a, &'a str, Vec<BucketSpec>, Extra<'a>> + Clone {
    bucket_spec().separated_by(sym(",")).at_least(1).collect()
}

fn bucket_fn_call<'a>() -> impl Parser<'a, &'a str, (BucketType, Vec<BucketSpec>), Extra<'a>> + Clone
{
    let with_conversion = kw("interpolate_cumulative_histogram")
        .ignore_then(sym("("))
        .ignore_then(bucket_conversion())
        .then_ignore(sym(","))
        .then(bucket_specs())
        .then_ignore(sym(")"))
        .map(|(mode, spec)| (BucketType::InterpolateCumulativeHistogram(mode), spec));
    let simple = choice((
        kw("histogram").to(BucketType::Histogram),
        kw("interpolate_delta_histogram").to(BucketType::InterpolateDeltaHistogram),
    ))
    .then(sym("(").ignore_then(bucket_specs()).then_ignore(sym(")")))
    .map(|(function, spec)| (function, spec));
    choice((with_conversion, simple))
}

/// `bucket ("by" <tags>)? ("to" <time>)? "using" <bucket_fn_call>`.
fn bucket_by<'a>() -> impl Parser<'a, &'a str, crate::query::BucketBy, Extra<'a>> + Clone {
    kw("bucket")
        .ignore_then(kw("by").ignore_then(tags()).or_not())
        .then(kw("to").ignore_then(parameterized_relative_time()).or_not())
        .then_ignore(kw("using"))
        .then(bucket_fn_call())
        .map_with(
            |((tags, time), (function, spec)), e| crate::query::BucketBy {
                span: to_span(e.span()),
                function,
                time,
                tags: tags.unwrap_or_default(),
                spec,
            },
        )
}

/// `join <tags> from <metric_id> by <tags>` — parsed but `NotSupported`.
fn join_clause<'a>() -> impl Parser<'a, &'a str, Clause, Extra<'a>> + Clone {
    kw("join")
        .ignore_then(tags())
        .then_ignore(kw("from"))
        .then(metric_id())
        .then_ignore(kw("by"))
        .then(tags())
        .validate(|_, e, emitter| {
            emitter.emit(rich(
                to_span(e.span()),
                Sem::NotSupported {
                    span: to_span(e.span()),
                    feature: "join".to_string(),
                },
            ));
            Clause::Skipped
        })
}

/// `replace (tag = tag ~ #s/…/…/ | tag ~ #s/…/…/ | tag = tag)` — parsed but
/// `NotSupported`.
fn replace_clause<'a>() -> impl Parser<'a, &'a str, Clause, Extra<'a>> + Clone {
    let rename_tag = ident()
        .then_ignore(sym("="))
        .then_ignore(ident())
        .then_ignore(sym("~"))
        .then_ignore(regex_replace_lit())
        .ignored();
    let tag = ident()
        .then_ignore(sym("~"))
        .then_ignore(regex_replace_lit())
        .ignored();
    let rename = ident().then_ignore(sym("=")).then_ignore(ident()).ignored();
    kw("replace")
        .ignore_then(choice((rename_tag, tag, rename)))
        .validate(|(), e, emitter| {
            emitter.emit(rich(
                to_span(e.span()),
                Sem::NotSupported {
                    span: to_span(e.span()),
                    feature: "replace".to_string(),
                },
            ));
            Clause::Skipped
        })
}

fn extend_expr<'a>() -> impl Parser<'a, &'a str, TagExtend, Extra<'a>> + Clone {
    ident()
        .then_ignore(sym("="))
        .then(expr())
        .map(|(tag, value)| TagExtend { tag, value })
}

fn extend_clause<'a>() -> impl Parser<'a, &'a str, Vec<TagExtend>, Extra<'a>> + Clone {
    kw("extend").ignore_then(extend_expr().separated_by(sym(",")).at_least(1).collect())
}

fn sample_clause<'a>() -> impl Parser<'a, &'a str, f64, Extra<'a>> + Clone {
    kw("sample").ignore_then(number_f64())
}

/// `ifdef($p) { <filter_expr> } (else { <filter_expr> })?`.
fn ifdef_clause<'a>() -> impl Parser<'a, &'a str, FilterOrIfDef, Extra<'a>> + Clone {
    let param = resolved_param().validate(|(span, param), _e, emitter| {
        if !param.is_optional() {
            emitter.emit(rich(
                span,
                Sem::IfdefNotOptional {
                    span,
                    param: param.clone(),
                },
            ));
        }
        param
    });
    let else_branch = kw("else")
        .ignore_then(sym("{"))
        .ignore_then(filter_expr())
        .then_ignore(sym("}"));
    kw("ifdef")
        .ignore_then(sym("("))
        .ignore_then(param)
        .then_ignore(sym(")"))
        .then_ignore(sym("{"))
        .then(filter_expr())
        .then_ignore(sym("}"))
        .then(else_branch.or_not())
        .map(|((param, filter), else_filter)| FilterOrIfDef::Ifdef {
            param,
            filter,
            else_filter,
        })
}

/// The pipe rules shared by simple queries and compute tails (no filter /
/// sample / ifdef): align, map, group, bucket, replace, join, extend, as.
fn pipe_rule_body<'a>() -> impl Parser<'a, &'a str, Clause, Extra<'a>> + Clone {
    choice((
        align().map(|a| Clause::Agg(Aggregate::Align(a))),
        map_clause().map(|m| Clause::Agg(Aggregate::Map(m))),
        group_by().map(|g| Clause::Agg(Aggregate::GroupBy(g))),
        bucket_by().map(|b| Clause::Agg(Aggregate::Bucket(b))),
        replace_clause(),
        join_clause(),
        extend_clause().map(Clause::Extend),
        as_clause().map(|a| Clause::Agg(Aggregate::As(a))),
    ))
}

fn simple_pipe_body<'a>() -> impl Parser<'a, &'a str, Clause, Extra<'a>> + Clone {
    choice((
        filter_expr().map(|f| Clause::Filter(FilterOrIfDef::Filter(f))),
        ifdef_clause().map(Clause::Filter),
        sample_clause().map(Clause::Sample),
        pipe_rule_body(),
    ))
}

/// Wraps a clause body with the leading `|` and clause-level recovery: a
/// malformed clause is consumed up to the next `|` (or EOF) and skipped, so one
/// bad pipe rule does not abort the whole parse.
fn clause<'a>(
    body: impl Parser<'a, &'a str, Clause, Extra<'a>> + Clone,
) -> impl Parser<'a, &'a str, Clause, Extra<'a>> + Clone {
    let real = sym("|").ignore_then(body);
    let recovery = sym("|")
        .ignore_then(none_of('|').repeated())
        .to(Clause::Skipped);
    real.recover_with(via_parser(recovery))
}

fn build_simple(
    source: Source,
    src_as: Option<As>,
    clauses: Vec<Clause>,
    directives: Directives,
    params: Params,
) -> Query {
    let mut filters = Vec::new();
    let mut aggregates = Vec::new();
    let mut extends = Vec::new();
    let mut sample = None;
    if let Some(a) = src_as {
        aggregates.push(Aggregate::As(a));
    }
    for c in clauses {
        match c {
            Clause::Filter(f) => filters.push(f),
            Clause::Agg(a) => aggregates.push(a),
            Clause::Extend(mut e) => extends.append(&mut e),
            Clause::Sample(s) => {
                if sample.is_none() {
                    sample = Some(s);
                }
            }
            Clause::Skipped => {}
        }
    }
    Query::Simple {
        source,
        filters,
        aggregates,
        directives,
        params,
        extends,
        sample,
    }
}

fn simple_query<'a>() -> impl Parser<'a, &'a str, Query, Extra<'a>> + Clone {
    source()
        .then(clause(simple_pipe_body()).repeated().collect::<Vec<_>>())
        .map_with(|((source, src_as), clauses), e| {
            let st = e.state();
            build_simple(
                source,
                src_as,
                clauses,
                st.directives.clone(),
                st.params.clone(),
            )
        })
}

/// `| compute <metric_name> using <compute_fn>`.
fn compute_rule<'a>() -> impl Parser<'a, &'a str, (Metric, ComputeFunction), Extra<'a>> + Clone {
    sym("|")
        .ignore_then(kw("compute"))
        .ignore_then(metric_name())
        .then_ignore(kw("using"))
        .then(compute_func())
}

#[allow(clippy::too_many_arguments)]
fn build_compute(
    left: Query,
    right: Query,
    name: Metric,
    op: ComputeFunction,
    clauses: Vec<Clause>,
    directives: Directives,
    params: Params,
) -> Query {
    let mut aggregates = Vec::new();
    let mut extends = Vec::new();
    for c in clauses {
        match c {
            Clause::Agg(a) => aggregates.push(a),
            Clause::Extend(mut e) => extends.append(&mut e),
            Clause::Filter(_) | Clause::Sample(_) | Clause::Skipped => {}
        }
    }
    Query::Compute {
        left: Box::new(left),
        right: Box::new(right),
        name,
        op,
        aggregates,
        extends,
        directives,
        params,
    }
}

/// `( <query> , <query> ,? ) <compute_rule> <pipe_rule>*`.
fn compute_query<'a>(
    query: impl Parser<'a, &'a str, Query, Extra<'a>> + Clone + 'a,
) -> impl Parser<'a, &'a str, Query, Extra<'a>> + Clone {
    sym("(")
        .ignore_then(query.clone())
        .then_ignore(sym(","))
        .then(query)
        .then_ignore(sym(",").or_not())
        .then_ignore(sym(")"))
        .then(compute_rule())
        .then(clause(pipe_rule_body()).repeated().collect::<Vec<_>>())
        .map_with(|(((left, right), (name, op)), clauses), e| {
            let st = e.state();
            build_compute(
                left,
                right,
                name,
                op,
                clauses,
                st.directives.clone(),
                st.params.clone(),
            )
        })
}

fn query<'a>() -> impl Parser<'a, &'a str, Query, Extra<'a>> + Clone {
    recursive(|query| choice((compute_query(query.clone()), simple_query())).boxed())
}

/// `param $name : type ;` — pushes the declaration into parser state so later
/// `$name` references resolve, with duplicate detection.
fn param_decl<'a>() -> impl Parser<'a, &'a str, (), Extra<'a>> + Clone {
    let name = lex(just('$')
        .ignore_then(plain_ident_raw())
        .map_with(|name, e| (name, to_span(e.span()))));
    kw("param")
        .ignore_then(name)
        .then_ignore(sym(":"))
        .then(param_type())
        .then_ignore(sym(";"))
        .validate(|((name, span), typ), e, emitter| {
            let st = e.state();
            if name.starts_with("__") {
                st.warnings.push_span(
                    span,
                    WarningReason::ParamUsingSystemPrefix {
                        param: name.clone(),
                    },
                );
            } else if st.params.iter().any(|p| p.name == name) {
                emitter.emit(rich(
                    span,
                    Sem::ParamDefinedMultipleTimes {
                        span,
                        param: name.clone(),
                    },
                ));
                return;
            }
            st.params.push(ParamDeclaration { span, name, typ });
        })
}

fn terminal_type<'a>() -> impl Parser<'a, &'a str, TerminalParamType, Extra<'a>> + Clone {
    choice((
        kw("string").to(TerminalParamType::Tag(TagType::String)),
        kw("int").to(TerminalParamType::Tag(TagType::Int)),
        kw("float").to(TerminalParamType::Tag(TagType::Float)),
        kw("bool").to(TerminalParamType::Tag(TagType::Bool)),
        kw("Dataset").to(TerminalParamType::Dataset),
        kw("Duration").to(TerminalParamType::Duration),
        kw("Regex").to(TerminalParamType::Regex),
        // deprecated lowercase alias — warn, like the old path.
        kw("duration").map_with(|(), e| {
            let span = to_span(e.span());
            e.state()
                .warnings
                .push_span(span, WarningReason::OldDuration);
            TerminalParamType::Duration
        }),
    ))
}

fn param_type<'a>() -> impl Parser<'a, &'a str, ParamType, Extra<'a>> + Clone {
    choice((
        kw("Option")
            .ignore_then(sym("<"))
            .ignore_then(terminal_type())
            .then_ignore(sym(">"))
            .validate(|t, e, emitter| {
                if matches!(t, TerminalParamType::Duration | TerminalParamType::Dataset) {
                    emitter.emit(rich(
                        to_span(e.span()),
                        Sem::Syntax {
                            span: to_span(e.span()),
                            label: "Option<…> only supports tag types and Regex".to_string(),
                            message: "Option<…> only supports tag types and Regex".to_string(),
                        },
                    ));
                }
                ParamType::Optional(t)
            }),
        terminal_type().map(ParamType::Terminal),
    ))
}

/// `set name (= value)? ;`.
fn directive<'a>() -> impl Parser<'a, &'a str, (), Extra<'a>> + Clone {
    let value = choice((
        plain_string().map(DirectiveValue::String),
        bool_lit().map(DirectiveValue::Bool),
        number_const().map(|tv| match tv {
            TagValue::Int(i) => DirectiveValue::Int(i),
            TagValue::Float(f) => DirectiveValue::Float(f),
            _ => DirectiveValue::None,
        }),
        ident().map(DirectiveValue::Ident),
    ));
    kw("set")
        .ignore_then(ident())
        .then(sym("=").ignore_then(value).or_not())
        .then_ignore(sym(";"))
        .map_with(|(name, value), e| {
            e.state()
                .directives
                .insert(name, value.unwrap_or(DirectiveValue::None));
        })
}

/// `file = directive* param* query EOI`. Trailing junk is reported but does not
/// discard the recovered query.
///
/// NB: `directive` and `param_decl` mutate parser state. `chumsky` parses the
/// *ignored* side of `ignore_then`/`then_ignore` in check mode, which skips
/// side effects — so the stateful preamble is kept on the emit path via `.then`
/// and discarded only afterwards with `.map`.
fn file<'a>() -> impl Parser<'a, &'a str, Query, Extra<'a>> {
    let trailing = any().repeated().to_slice().validate(|s: &str, e, emitter| {
        if !s.trim().is_empty() {
            emitter.emit(rich(
                to_span(e.span()),
                Sem::Syntax {
                    span: to_span(e.span()),
                    label: "unexpected trailing input".to_string(),
                    message: format!("unexpected trailing input: {:?}", s.trim()),
                },
            ));
        }
    });
    let preamble = directive()
        .repeated()
        .collect::<Vec<_>>()
        .then(param_decl().repeated().collect::<Vec<_>>());
    trivia()
        .ignore_then(preamble)
        .then(query())
        .then(trailing)
        .map(|((_preamble, query), ())| query)
        .then_ignore(end())
}

// ─────────────────────── external param-value entry ───────────────────────

/// Runs `parser` over the whole of `src` (ignoring any trailing input, the way
/// the old single-rule `pest` entry point did) returning the produced value
/// only on a clean parse.
fn run_value<'a, O>(parser: impl Parser<'a, &'a str, O, Extra<'a>>, src: &'a str) -> Option<O> {
    let mut st: St = SimpleState(SliceState::default());
    parser
        .then_ignore(any().repeated())
        .parse_with_state(src, &mut st)
        .into_result()
        .ok()
}

/// Parses a host-supplied param value string according to the declared param
/// type — the external entry point that was `Rule::param_value`.
pub fn parse_param_value(
    param: &ParamDeclaration,
    src: &str,
) -> Result<ParamValue, ParseParamError> {
    let mismatch = || ParseParamError::TypeMismatch {
        declared_typ: param.typ,
        found: src.trim().to_string(),
    };
    match param.typ() {
        TerminalParamType::Dataset => {
            run_value(choice((escaped_ident_raw(), plain_ident_raw())), src)
                .map(|s| ParamValue::Dataset(Dataset::new(s)))
                .ok_or_else(mismatch)
        }
        TerminalParamType::Duration => run_value(relative_time(), src)
            .map(ParamValue::Duration)
            .ok_or_else(mismatch),
        TerminalParamType::Regex => run_value(regex_lit(), src)
            .map(ParamValue::Regex)
            .ok_or_else(mismatch),
        TerminalParamType::Tag(TagType::String) => run_value(plain_string(), src)
            .map(ParamValue::String)
            .ok_or_else(mismatch),
        TerminalParamType::Tag(TagType::Int) => {
            let digit = any().filter(|c: &char| c.is_ascii_digit());
            let int = lex(one_of("+-")
                .or_not()
                .then(digit.repeated().at_least(1))
                .then_ignore(just('.').not())
                .to_slice());
            run_value(int, src)
                .and_then(|s: &str| s.trim().parse::<i64>().ok())
                .map(ParamValue::Int)
                .ok_or_else(mismatch)
        }
        TerminalParamType::Tag(TagType::Float) => run_value(number_const(), src)
            .and_then(|tv| match tv {
                TagValue::Float(f) => Some(ParamValue::Float(f)),
                TagValue::Null | TagValue::Int(_) | TagValue::Bool(_) | TagValue::String(_) => None,
            })
            .ok_or_else(mismatch),
        TerminalParamType::Tag(TagType::Bool) => run_value(bool_lit(), src)
            .map(ParamValue::Bool)
            .ok_or_else(mismatch),
        TerminalParamType::Tag(TagType::Null) => Err(ParseParamError::NoneParam),
    }
}

// ───────────────────────────── shared helpers ─────────────────────────────

/// Unescapes the inner text of a string / backtick identifier. This is the
/// crate's single canonical unescape (the old `pest` path had a duplicate).
fn unescape(data: &str, delim: char) -> String {
    let mut escaped = false;
    let mut res = String::with_capacity(data.len());
    for c in data.chars() {
        if escaped {
            escaped = false;
            match c {
                'r' => res.push('\r'),
                'n' => res.push('\n'),
                't' => res.push('\t'),
                'b' => res.push('\u{08}'),
                'f' => res.push('\u{0C}'),
                '\\' => res.push('\\'),
                '$' => res.push('$'),
                c if c == delim => res.push(delim),
                _ => {
                    res.push('\\');
                    res.push(c);
                }
            }
        } else if c == '\\' {
            escaped = true;
        } else {
            res.push(c);
        }
    }
    res
}
