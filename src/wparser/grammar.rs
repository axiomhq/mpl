//! Structural `winnow` combinators that build the [`crate::query`] AST for the
//! whole MPL grammar, with pipe-boundary error recovery.
//!
//! ## Trivia
//! Comments (`// …`) and whitespace are *skipped* between tokens by [`trivia`]
//! (the same role pest's silent `WHITESPACE`/`COMMENT` rules played). The
//! lossless, formatter-ready view of trivia lives in the [`super::lex`] layer,
//! which preserves every byte.
//!
//! ## State
//! Declared params (for `$x` resolution), the directive map and a diagnostics
//! sink are threaded through the parser via `winnow`'s [`Stateful`] stream.
//!
//! ## Error recovery
//! `winnow` has no built-in recovery on stable, so [`parse_simple_query`]
//! splits the query at top-level `|` pipes and parses each clause
//! independently: a malformed clause records a diagnostic and resyncs to the
//! next `|` (or `,`/`)` inside a `compute` sub-query) instead of sinking the
//! whole parse.

use std::{cell::RefCell, ops::Range};

use chrono::DateTime;
use miette::SourceSpan;
use strumbra::SharedString;
use winnow::{
    ModalResult, Parser,
    ascii::{digit0, digit1, multispace1, till_line_ending},
    combinator::{alt, cut_err, eof, not, opt, preceded, repeat, separated},
    error::{ContextError, ErrMode, StrContext, StrContextValue},
    stream::{LocatingSlice, Location, Stateful, Stream},
    token::{any, literal, none_of, one_of, take_till, take_while},
};

use crate::{
    ParseError,
    enc_regex::EncodableRegex,
    errors::{ParseParamError, UnsupportedRule},
    linker::{AlignFunction, ComputeFunction, Function, FunctionId, GroupFunction, ModuleId},
    query::{
        Aggregate, Align, As, BucketBy, Cmp, DirectiveValue, Directives, Expr, Filter,
        FilterOrIfDef, GroupBy, Mapping, MetricId, ParamDeclaration, ParamType, ParamValue, Query,
        RelativeTime, Source, StringFragment, TagExtend, TagType, TerminalParamType, Time,
        TimeRange, TimeUnit, WarningReason, Warnings,
    },
    stdlib::STDLIB,
    tags::TagValue,
    types::{BucketSpec, BucketType, ConversionMethod, Dataset, Metric, Parameterized},
};

const SYSTEM_PARAM_PREFIX: &str = "__";

/// The parser stream: a span-tracking `&str` carrying the [`Ctx`] state.
type Input<'s> = Stateful<LocatingSlice<&'s str>, Ctx<'s>>;
type PResult<T> = ModalResult<T>;

// ── escaping (the canonical leaf helpers, formerly in `parser.rs`) ───

/// Strip the surrounding `delim` characters and unescape the inner text.
pub(crate) fn unescape_and_trim(data: &str, delim: char) -> String {
    unescape(
        data.trim_start_matches(delim).trim_end_matches(delim),
        delim,
    )
}

/// Resolve `\`-escape sequences (`\n`, `\t`, `\\`, `\$`, `\<delim>`, …).
pub(crate) fn unescape(data: &str, delim: char) -> String {
    let mut escaped = false;
    let mut res = String::with_capacity(data.len());
    for c in data.chars() {
        if escaped {
            escaped = false;
            match c {
                'r' => res.push('\r'),
                'n' => res.push('\n'),
                't' => res.push('\t'),
                'b' => res.push('\x08'),
                'f' => res.push('\x0C'),
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

/// Diagnostics collected during a (recovering) parse.
#[derive(Debug, Default)]
struct Diags {
    errors: Vec<ParseError>,
    warnings: Warnings,
}

/// Parser state: declared params (for `$x` resolution), parsed directives, and
/// the diagnostics sink. `Copy` so it threads cheaply through every combinator.
#[derive(Debug, Clone, Copy)]
struct Ctx<'s> {
    params: &'s [ParamDeclaration],
    directives: &'s Directives,
    diags: &'s RefCell<Diags>,
}

impl Ctx<'_> {
    fn push_err(self, err: ParseError) {
        self.diags.borrow_mut().errors.push(err);
    }

    fn push_warning(self, span: SourceSpan, reason: WarningReason) {
        self.diags.borrow_mut().warnings.push_span(span, reason);
    }

    fn error_count(self) -> usize {
        self.diags.borrow().errors.len()
    }

    /// Resolve a `$name` reference to its declaration, recording an
    /// [`ParseError::UndefinedParam`] (and returning `None`) when unknown.
    fn resolve(self, name: &str, span: SourceSpan) -> Option<ParamDeclaration> {
        let Some(decl) = self.params.iter().find(|p| p.name == name) else {
            self.push_err(ParseError::UndefinedParam {
                span,
                param: name.to_string(),
            });
            return None;
        };
        Some(decl.clone())
    }
}

/// The result of [`parse_file`]: a best-effort AST plus all diagnostics. With
/// recovery, `query` can be `Some` even when `errors` is non-empty.
#[derive(Debug)]
pub struct ParseOutput {
    /// The parsed query (best effort; `None` only when the source itself
    /// could not be parsed).
    pub query: Option<Query>,
    /// Every parse/semantic error encountered (multi-error).
    pub errors: Vec<ParseError>,
    /// Non-fatal warnings (e.g. legacy `duration`).
    pub warnings: Warnings,
}

/// A recoverable failure that has already recorded its own typed diagnostic.
fn recorded() -> ErrMode<ContextError> {
    ErrMode::Cut(ContextError::new())
}

fn backtrack() -> ErrMode<ContextError> {
    ErrMode::Backtrack(ContextError::new())
}

fn to_span(range: Range<usize>) -> SourceSpan {
    SourceSpan::new(range.start.into(), range.len())
}

/// `literal` with the error type pinned to [`ContextError`].
fn lit<'s>(input: &mut Input<'s>, value: &'static str) -> PResult<&'s str> {
    literal(value).parse_next(input)
}

fn at_eof(input: &mut Input) -> bool {
    eof::<_, ContextError>.parse_next(input).is_ok()
}

/// The next character without consuming it (for boundary checks).
fn peek_char(input: &Input) -> Option<char> {
    input.input.chars().next()
}

/// A span covering `start..` the current offset.
fn span_from(input: &Input, start: usize) -> SourceSpan {
    to_span(start..input.current_token_start())
}

// ── trivia & low-level tokens ───────────────────────────────────

fn line_comment(input: &mut Input) -> PResult<()> {
    (literal("//"), till_line_ending).void().parse_next(input)
}

/// Skip whitespace and `// …` comments between tokens.
fn trivia(input: &mut Input) -> PResult<()> {
    repeat(0.., alt((multispace1.void(), line_comment))).parse_next(input)
}

/// Offset of the next token, after skipping trivia.
fn token_start(input: &mut Input) -> usize {
    let _ = trivia(input);
    input.current_token_start()
}

fn ident_cont(input: &mut Input) -> PResult<char> {
    one_of(|c: char| c.is_ascii_alphanumeric() || c == '_').parse_next(input)
}

/// Match `word` as a whole keyword (trivia-prefixed, word-boundary-terminated).
/// Atomic: consumes nothing on failure, so it is safe directly or in `alt`.
fn keyword(input: &mut Input, word: &'static str) -> PResult<()> {
    let start = input.checkpoint();
    let _ = trivia(input);
    let matched = lit(input, word).is_ok() && not(ident_cont).parse_next(input).is_ok();
    if matched {
        Ok(())
    } else {
        input.reset(&start);
        Err(backtrack())
    }
}

/// Match a punctuation/symbol literal (trivia-prefixed). Atomic.
fn symbol(input: &mut Input, sym: &'static str) -> PResult<()> {
    let start = input.checkpoint();
    let _ = trivia(input);
    if lit(input, sym).is_ok() {
        Ok(())
    } else {
        input.reset(&start);
        Err(backtrack())
    }
}

// ── identifiers ─────────────────────────────────────────────────

/// A plain identifier core (no trivia, no escaping): `(alpha|_)(alnum|_)*`.
fn plain_ident<'s>(input: &mut Input<'s>) -> PResult<&'s str> {
    (
        one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_till(0.., |c: char| !(c.is_ascii_alphanumeric() || c == '_')),
    )
        .take()
        .parse_next(input)
}

/// A backtick-escaped identifier core, returning the unescaped name.
fn escaped_ident(input: &mut Input) -> PResult<String> {
    let raw = (
        literal("`"),
        repeat::<_, _, (), _, _>(
            0..,
            alt((
                preceded(literal("\\"), any).void(),
                none_of(['`', '\\']).void(),
            )),
        ),
        literal("`"),
    )
        .take()
        .parse_next(input)?;
    Ok(unescape_and_trim(raw, '`'))
}

/// Any identifier (plain or escaped) core, returning the unescaped name.
fn ident_name(input: &mut Input) -> PResult<String> {
    alt((plain_ident.map(str::to_string), escaped_ident)).parse_next(input)
}

/// A trivia-prefixed identifier (a `tag`/field name).
fn tag_name(input: &mut Input) -> PResult<String> {
    let _ = trivia(input);
    ident_name.parse_next(input)
}

/// `tags`: one or more comma-separated identifiers.
fn tags(input: &mut Input) -> PResult<Vec<String>> {
    let mut out = vec![tag_name(input)?];
    while opt(|i: &mut Input| symbol(i, ","))
        .parse_next(input)?
        .is_some()
    {
        out.push(tag_name(input)?);
    }
    Ok(out)
}

/// A trivia-prefixed identifier with its source span.
fn ident_spanned(input: &mut Input) -> PResult<(String, SourceSpan)> {
    let _ = trivia(input);
    ident_name
        .with_span()
        .map(|(name, range)| (name, to_span(range)))
        .parse_next(input)
}

/// A `$name` reference. Backtracks (consuming nothing) when there is no `$`.
/// The returned span covers `$name`.
fn param_ref(input: &mut Input) -> PResult<(String, SourceSpan)> {
    let start = input.checkpoint();
    let _ = trivia(input);
    let Ok((name, range)) = preceded(literal("$"), ident_name)
        .with_span()
        .parse_next(input)
    else {
        input.reset(&start);
        return Err(backtrack());
    };
    Ok((name, to_span(range)))
}

/// A `$name` reference resolved to its declaration. Backtracks when there is no
/// `$`; records [`ParseError::UndefinedParam`] and cuts when `$name` is unknown.
fn param_decl_ref(input: &mut Input) -> PResult<(SourceSpan, ParamDeclaration)> {
    let (name, span) = param_ref(input)?;
    match input.state.resolve(&name, span) {
        Some(decl) => Ok((span, decl)),
        None => Err(recorded()),
    }
}

// ── metric source ───────────────────────────────────────────────

/// A dataset (concrete ident or `$param`) plus its source span.
fn dataset_spanned(input: &mut Input) -> PResult<(Parameterized<Dataset>, SourceSpan)> {
    alt((
        param_decl_ref.map(|(span, param)| (Parameterized::Param { span, param }, span)),
        ident_spanned.map(|(name, span)| (Parameterized::Concrete(Dataset::new(name)), span)),
    ))
    .parse_next(input)
}

/// A metric name. Records its own typed error (and cuts) when the name is
/// missing/invalid, mirroring pest's farthest-failure span.
fn metric_name(input: &mut Input) -> PResult<Metric> {
    let at = token_start(input);
    let Ok((name, range)) = ident_name.with_span().parse_next(input) else {
        let len = usize::from(!at_eof(input));
        input.state.push_err(ParseError::SyntaxError {
            span: to_span(at..at + len),
            label: "metric name".to_string(),
            message: "expected a metric name".to_string(),
            suggestion: None,
        });
        return Err(recorded());
    };
    match Metric::try_from(name) {
        Ok(metric) => Ok(metric),
        Err(err) => {
            input.state.push_err(ParseError::SyntaxError {
                span: to_span(range),
                label: "invalid metric name".to_string(),
                message: err.to_string(),
                suggestion: None,
            });
            Err(recorded())
        }
    }
}

fn metric_id(input: &mut Input) -> PResult<MetricId> {
    let (dataset, ds_span) = dataset_spanned(input)?;
    if symbol(input, ":").is_err() {
        input.state.push_err(ParseError::SyntaxError {
            span: ds_span,
            label: "metric identifier (e.g., dataset:metric)".to_string(),
            message: "expected `:` and a metric name after the dataset".to_string(),
            suggestion: None,
        });
        return Err(recorded());
    }
    let metric = metric_name(input)?;
    Ok(MetricId { dataset, metric })
}

fn source(input: &mut Input) -> PResult<(Source, Option<As>)> {
    let metric_id = metric_id(input)?;
    let time = opt(time_range).parse_next(input)?;
    let as_ = opt(as_rename).parse_next(input)?;
    Ok((Source { metric_id, time }, as_))
}

fn as_rename(input: &mut Input) -> PResult<As> {
    keyword(input, "as")?;
    let name = cut_err(metric_name)
        .context(StrContext::Label("metric name after `as`"))
        .parse_next(input)?;
    Ok(As { name })
}

// ── time ────────────────────────────────────────────────────────

fn relative_time(input: &mut Input) -> PResult<RelativeTime> {
    let _ = trivia(input);
    let value = digit1.parse_to::<u64>().parse_next(input)?;
    let unit = alt((
        literal("ms").value(TimeUnit::Millisecond),
        literal("s").value(TimeUnit::Second),
        literal("m").value(TimeUnit::Minute),
        literal("h").value(TimeUnit::Hour),
        literal("d").value(TimeUnit::Day),
        literal("w").value(TimeUnit::Week),
        literal("M").value(TimeUnit::Month),
        literal("y").value(TimeUnit::Year),
    ))
    .parse_next(input)?;
    Ok(RelativeTime { value, unit })
}

/// `time_relative_parameterized`: a relative time or a `$dur` param.
fn param_relative_time(input: &mut Input) -> PResult<Parameterized<RelativeTime>> {
    alt((
        param_decl_ref.map(|(span, param)| Parameterized::Param { span, param }),
        relative_time.map(Parameterized::Concrete),
    ))
    .parse_next(input)
}

fn n_digits<'s>(n: usize) -> impl FnMut(&mut Input<'s>) -> PResult<&'s str> {
    move |input: &mut Input<'s>| take_while(n..=n, |c: char| c.is_ascii_digit()).parse_next(input)
}

/// `2025-03-01T13:00:00Z` (RFC 3339).
fn time_rfc3339(input: &mut Input) -> PResult<Time> {
    let _ = trivia(input);
    let raw = (
        (
            n_digits(4),
            literal("-"),
            n_digits(2),
            literal("-"),
            n_digits(2),
        ),
        (
            literal("T"),
            n_digits(2),
            literal(":"),
            n_digits(2),
            literal(":"),
            n_digits(2),
        ),
        opt(literal("Z")),
    )
        .take()
        .parse_next(input)?;
    match DateTime::parse_from_rfc3339(raw) {
        Ok(dt) => Ok(Time::RFC3339(dt)),
        Err(err) => {
            input.state.push_err(ParseError::from(err));
            Err(recorded())
        }
    }
}

/// `+1h` / `-30m` time modifier (stored verbatim).
fn time_modifier(input: &mut Input) -> PResult<Time> {
    let _ = trivia(input);
    let raw = (one_of(['+', '-']), relative_time)
        .take()
        .parse_next(input)?;
    Ok(Time::Modifier(raw.to_string()))
}

/// A single `time`: relative, RFC3339, bare timestamp, or modifier.
fn time(input: &mut Input) -> PResult<Time> {
    alt((
        time_rfc3339,
        relative_time.map(Time::Relative),
        preceded(trivia, digit1.parse_to::<i64>()).map(Time::Timestamp),
        time_modifier,
    ))
    .parse_next(input)
}

/// `time_range`: `[<time>..]`, `[<time>..<time>]`.
fn time_range(input: &mut Input) -> PResult<TimeRange> {
    symbol(input, "[")?;
    let start = cut_err(time)
        .context(StrContext::Label("range start"))
        .parse_next(input)?;
    symbol(input, "..")?;
    let end = opt(time).parse_next(input)?;
    cut_err(|i: &mut Input| symbol(i, "]"))
        .context(StrContext::Expected(StrContextValue::CharLiteral(']')))
        .parse_next(input)?;
    Ok(TimeRange { start, end })
}

// ── expressions & literals ──────────────────────────────────────

fn bool_lit(input: &mut Input) -> PResult<bool> {
    let start = input.checkpoint();
    let _ = trivia(input);
    let value = if lit(input, "true").is_ok() {
        true
    } else if lit(input, "false").is_ok() {
        false
    } else {
        input.reset(&start);
        return Err(backtrack());
    };
    // word-boundary guard so `truely` is not read as `true`
    if not(ident_cont).parse_next(input).is_ok() {
        Ok(value)
    } else {
        input.reset(&start);
        Err(backtrack())
    }
}

/// A raw double-quoted string literal, returning its unescaped text (no
/// interpolation). Used by directives and `param_value`.
fn string_raw(input: &mut Input) -> PResult<String> {
    let _ = trivia(input);
    let raw = (
        literal("\""),
        repeat::<_, _, (), _, _>(
            0..,
            alt((
                preceded(literal("\\"), any).void(),
                none_of(['"', '\\']).void(),
            )),
        ),
        literal("\""),
    )
        .take()
        .parse_next(input)?;
    Ok(unescape_and_trim(raw, '"'))
}

/// A double-quoted string, split into [`StringFragment`]s on `${ expr }`
/// interpolation. Collapses to a plain constant when there is no interpolation
/// (matching pest's `parse_string`).
fn string_expr(input: &mut Input) -> PResult<Expr> {
    let _ = trivia(input);
    literal("\"").parse_next(input)?;
    let mut frags: Vec<StringFragment> = Vec::new();
    let mut raw = String::new();

    loop {
        if opt(literal("\"")).parse_next(input)?.is_some() {
            break;
        }
        if opt(literal("${")).parse_next(input)?.is_some() {
            if !raw.is_empty() {
                frags.push(StringFragment::Text(unescape(&raw, '"')));
                raw.clear();
            }
            let _ = trivia(input);
            let value = expr(input)?;
            let _ = trivia(input);
            cut_err(|i: &mut Input| symbol(i, "}"))
                .context(StrContext::Expected(StrContextValue::CharLiteral('}')))
                .parse_next(input)?;
            frags.push(StringFragment::Expr(value));
            continue;
        }
        if opt(literal("\\")).parse_next(input)?.is_some() {
            raw.push('\\');
            raw.push(any.parse_next(input)?);
            continue;
        }
        raw.push(none_of(['"', '\\']).parse_next(input)?);
    }

    if !raw.is_empty() {
        frags.push(StringFragment::Text(unescape(&raw, '"')));
    }
    if frags.iter().all(|f| matches!(f, StringFragment::Text(_))) {
        let text: String = frags
            .into_iter()
            .map(|f| match f {
                StringFragment::Text(t) => t,
                StringFragment::Expr(_) => String::new(),
            })
            .collect();
        let shared = SharedString::try_from(text).map_err(|err| {
            input.state.push_err(ParseError::from(err));
            recorded()
        })?;
        return Ok(Expr::Const(TagValue::String(shared)));
    }
    Ok(Expr::String(frags))
}

/// A numeric literal. Float iff it has a fractional/exponent part.
fn number_lit(input: &mut Input) -> PResult<TagValue> {
    let _ = trivia(input);
    let raw = (
        opt(one_of(['+', '-'])),
        digit1,
        opt((
            literal("."),
            digit0,
            opt((one_of(['e', 'E']), opt(one_of(['+', '-'])), digit1)),
        )),
    )
        .take()
        .parse_next(input)?;
    if raw.contains('.') {
        raw.parse::<f64>()
            .map(TagValue::Float)
            .map_err(|_| backtrack())
    } else {
        raw.parse::<i64>()
            .map(TagValue::Int)
            .map_err(|_| backtrack())
    }
}

fn inf_lit(input: &mut Input) -> PResult<f64> {
    let start = input.checkpoint();
    let _ = trivia(input);
    let value = if lit(input, "+inf").is_ok() || lit(input, "inf").is_ok() {
        f64::INFINITY
    } else if lit(input, "-inf").is_ok() {
        f64::NEG_INFINITY
    } else {
        input.reset(&start);
        return Err(backtrack());
    };
    if not(ident_cont).parse_next(input).is_ok() {
        Ok(value)
    } else {
        input.reset(&start);
        Err(backtrack())
    }
}

/// A `number` argument coerced to `f64` (used by `map`/`bucket`/`sample`).
#[allow(clippy::cast_precision_loss)]
fn number_arg(input: &mut Input) -> PResult<f64> {
    if let Some(value) = opt(inf_lit).parse_next(input)? {
        return Ok(value);
    }
    Ok(match number_lit(input)? {
        TagValue::Int(value) => value as f64,
        TagValue::Float(value) => value,
        _ => return Err(backtrack()),
    })
}

/// `expr = const | param_ident | ident` (with string interpolation).
fn expr(input: &mut Input) -> PResult<Expr> {
    if let Some((span, param)) = opt(param_decl_ref).parse_next(input)? {
        return Ok(Expr::Param { span, param });
    }
    if let Some(value) = opt(bool_lit).parse_next(input)? {
        return Ok(Expr::Const(TagValue::Bool(value)));
    }
    if let Some(value) = opt(inf_lit).parse_next(input)? {
        return Ok(Expr::Const(TagValue::Float(value)));
    }
    if let Some(value) = opt(number_lit).parse_next(input)? {
        return Ok(Expr::Const(value));
    }
    let _ = trivia(input);
    if peek_char(input) == Some('"') {
        return string_expr(input);
    }
    let (name, _) = ident_spanned(input)?;
    Ok(Expr::Tag(name))
}

/// `#/regex/` compiled to a concrete regex value.
fn regex_literal(input: &mut Input) -> PResult<EncodableRegex> {
    let _ = trivia(input);
    let raw = (
        literal("#/"),
        repeat::<_, _, (), _, _>(
            0..,
            alt((
                preceded(literal("\\"), any).void(),
                none_of(['/', '\\']).void(),
            )),
        ),
        literal("/"),
    )
        .take()
        .parse_next(input)?;
    // raw == "#/…/"; strip the leading '#', then trim the '/' delimiters.
    let pattern = unescape_and_trim(&raw[1..], '/');
    match regex::Regex::new(&pattern) {
        Ok(re) => Ok(EncodableRegex::from(re)),
        Err(err) => {
            input.state.push_err(ParseError::from(err));
            Err(recorded())
        }
    }
}

/// `#s/src/dst/` regex-replace literal. Parsed and discarded (the `replace`
/// pipe is recognised but unsupported by the backend).
fn regex_replace_literal(input: &mut Input) -> PResult<()> {
    let _ = trivia(input);
    (
        literal("#s/"),
        regex_replace_body,
        literal("/"),
        regex_replace_body,
        literal("/"),
    )
        .void()
        .parse_next(input)
}

fn regex_replace_body(input: &mut Input) -> PResult<()> {
    repeat(
        0..,
        alt((
            preceded(literal("\\"), any).void(),
            none_of(['/', '\\']).void(),
        )),
    )
    .parse_next(input)
}

// ── filters ─────────────────────────────────────────────────────

fn tag_type(input: &mut Input) -> PResult<TagType> {
    let (name, span) = ident_spanned(input)?;
    match name.as_str() {
        "string" => Ok(TagType::String),
        "int" => Ok(TagType::Int),
        "float" => Ok(TagType::Float),
        "bool" => Ok(TagType::Bool),
        other => {
            input.state.push_err(ParseError::InvalidTagType {
                span,
                tpe: other.to_string(),
            });
            Err(recorded())
        }
    }
}

fn comparison(input: &mut Input) -> PResult<&'static str> {
    let _ = trivia(input);
    alt((
        literal("==").value("=="),
        literal("!=").value("!="),
        literal("<=").value("<="),
        literal(">=").value(">="),
        literal("<").value("<"),
        literal(">").value(">"),
    ))
    .parse_next(input)
}

/// The right-hand side of a `filter_atom`: `is <type>`, `<cmp> <expr>`, or the
/// `== #/regex/` form. The `== $param` vs `== #/regex/` ambiguity that pest
/// defers to type-checking is handled the same way here: a `$param` parses as
/// `Cmp::Eq(Expr::Param)` and `ParamTypecheckVisitor` rewrites it to a regex
/// comparison when the param is `Regex`-typed.
fn filter_rhs(input: &mut Input) -> PResult<Cmp> {
    if opt(|i: &mut Input| keyword(i, "is"))
        .parse_next(input)?
        .is_some()
    {
        return Ok(Cmp::Is(tag_type(input)?));
    }
    let op = comparison(input)?;
    match op {
        "==" => regex_or_expr(input, true),
        "!=" => regex_or_expr(input, false),
        ">" => expr(input).map(Cmp::Gt),
        ">=" => expr(input).map(Cmp::Ge),
        "<" => expr(input).map(Cmp::Lt),
        "<=" => expr(input).map(Cmp::Le),
        _ => Err(recorded()),
    }
}

/// `== #/regex/` vs `== <expr>`. `eq` selects equality vs inequality.
fn regex_or_expr(input: &mut Input, eq: bool) -> PResult<Cmp> {
    if let Some(re) = opt(regex_literal).parse_next(input)? {
        let re = Parameterized::Concrete(re);
        return Ok(if eq {
            Cmp::RegEx(re)
        } else {
            Cmp::RegExNot(re)
        });
    }
    let value = expr(input)?;
    Ok(if eq { Cmp::Eq(value) } else { Cmp::Ne(value) })
}

fn filter_atom(input: &mut Input) -> PResult<Filter> {
    let (field, _) = ident_spanned(input)?;
    let rhs = filter_rhs(input)?;
    Ok(Filter::Cmp { field, rhs })
}

fn filter_clause(input: &mut Input) -> PResult<Filter> {
    if opt(|i: &mut Input| symbol(i, "("))
        .parse_next(input)?
        .is_some()
    {
        let inner = filter_or(input)?;
        cut_err(|i: &mut Input| symbol(i, ")"))
            .context(StrContext::Expected(StrContextValue::CharLiteral(')')))
            .parse_next(input)?;
        Ok(inner)
    } else {
        filter_atom(input)
    }
}

fn filter_not(input: &mut Input) -> PResult<Filter> {
    if opt(|i: &mut Input| keyword(i, "not"))
        .parse_next(input)?
        .is_some()
    {
        Ok(Filter::Not(Box::new(filter_clause(input)?)))
    } else {
        filter_clause(input)
    }
}

fn collapse(mut items: Vec<Filter>, combine: fn(Vec<Filter>) -> Filter) -> Filter {
    if items.len() == 1 {
        items.remove(0)
    } else {
        combine(items)
    }
}

fn filter_and(input: &mut Input) -> PResult<Filter> {
    let mut items = vec![filter_not(input)?];
    while opt(|i: &mut Input| keyword(i, "and"))
        .parse_next(input)?
        .is_some()
    {
        items.push(filter_not(input)?);
    }
    Ok(collapse(items, Filter::And))
}

fn filter_or(input: &mut Input) -> PResult<Filter> {
    let mut items = vec![filter_and(input)?];
    while opt(|i: &mut Input| keyword(i, "or"))
        .parse_next(input)?
        .is_some()
    {
        items.push(filter_and(input)?);
    }
    Ok(collapse(items, Filter::Or))
}

/// `filter`/`where` keyword (the two are aliases).
fn filter_keyword(input: &mut Input) -> PResult<()> {
    let start = input.checkpoint();
    let _ = trivia(input);
    let matched = (lit(input, "filter").is_ok() || lit(input, "where").is_ok())
        && not(ident_cont).parse_next(input).is_ok();
    if matched {
        Ok(())
    } else {
        input.reset(&start);
        Err(backtrack())
    }
}

/// `filter_keyword filter_or` — the body shared by `| filter` and `ifdef`.
fn where_part(input: &mut Input) -> PResult<Filter> {
    filter_keyword(input)?;
    cut_err(filter_or)
        .context(StrContext::Label("filter expression"))
        .parse_next(input)
}

fn filter_rule(input: &mut Input) -> PResult<FilterOrIfDef> {
    where_part(input).map(FilterOrIfDef::Filter)
}

/// `ifdef($p) { where … } (else { where … })?`.
fn ifdef_rule(input: &mut Input) -> PResult<FilterOrIfDef> {
    keyword(input, "ifdef")?;
    cut_err(ifdef_body)
        .context(StrContext::Label("ifdef clause"))
        .parse_next(input)
}

fn ifdef_body(input: &mut Input) -> PResult<FilterOrIfDef> {
    symbol(input, "(")?;
    let (span, param) = param_decl_ref(input)?;
    if !param.is_optional() {
        input
            .state
            .push_err(ParseError::IfdefNotOptional { span, param });
        return Err(recorded());
    }
    symbol(input, ")")?;
    symbol(input, "{")?;
    let filter = where_part(input)?;
    symbol(input, "}")?;
    let else_filter = if opt(|i: &mut Input| keyword(i, "else"))
        .parse_next(input)?
        .is_some()
    {
        symbol(input, "{")?;
        let f = where_part(input)?;
        symbol(input, "}")?;
        Some(f)
    } else {
        None
    };
    Ok(FilterOrIfDef::Ifdef {
        param,
        filter,
        else_filter,
    })
}

// ── functions ───────────────────────────────────────────────────

/// `func = (module "::")* ident`, returned with the source span.
fn function_id(input: &mut Input) -> PResult<(Function, SourceSpan)> {
    let _ = trivia(input);
    let (mut parts, range): (Vec<String>, Range<usize>) = separated(1.., ident_name, literal("::"))
        .with_span()
        .parse_next(input)?;
    let Some(name) = parts.pop() else {
        return Err(backtrack());
    };
    let module_path = parts.iter().map(|p| ModuleId::new(p)).collect();
    Ok((
        Function {
            name: FunctionId::new(&name),
            module_path,
        },
        to_span(range),
    ))
}

/// One of `+ - * /` (shared by `map_eval` and `compute_op`).
fn calc_op(input: &mut Input) -> PResult<&'static str> {
    let _ = trivia(input);
    alt((
        literal("+").value("+"),
        literal("-").value("-"),
        literal("*").value("*"),
        literal("/").value("/"),
    ))
    .parse_next(input)
}

// ── align / map / group / bucket ────────────────────────────────

fn align_function(input: &mut Input) -> PResult<AlignFunction> {
    let (func, span) = function_id(input)?;
    let Some(function) = STDLIB.align_fn(&func) else {
        input.state.push_err(ParseError::UnsupportedAlignFunction {
            span,
            name: func.to_string(),
        });
        return Err(recorded());
    };
    Ok(function.clone())
}

/// `align (to <reltime>)? (over <reltime>)? using <func>`.
fn align_rule(input: &mut Input) -> PResult<Aggregate> {
    keyword(input, "align")?;
    cut_err(align_body)
        .context(StrContext::Label("align clause"))
        .parse_next(input)
}

fn align_body(input: &mut Input) -> PResult<Aggregate> {
    let time = if opt(|i: &mut Input| keyword(i, "to"))
        .parse_next(input)?
        .is_some()
    {
        Some(param_relative_time(input)?)
    } else {
        None
    };
    let over = opt(|i: &mut Input| keyword(i, "over"))
        .parse_next(input)?
        .is_some();
    if over {
        // mirror pest: the sliding-window form parses but is unsupported
        let _ = param_relative_time(input)?;
    }
    keyword(input, "using")?;
    let function = align_function(input)?;
    if over {
        input
            .state
            .push_err(ParseError::NotImplemented("sliding windows"));
        return Err(recorded());
    }
    Ok(Aggregate::Align(Align { function, time }))
}

fn map_function(
    input: &mut Input,
    func: &Function,
    span: SourceSpan,
    eval: bool,
) -> PResult<Mapping> {
    let arg = if eval {
        None
    } else if opt(|i: &mut Input| symbol(i, "("))
        .parse_next(input)?
        .is_some()
    {
        let value = number_arg(input)?;
        cut_err(|i: &mut Input| symbol(i, ")"))
            .context(StrContext::Expected(StrContextValue::CharLiteral(')')))
            .parse_next(input)?;
        Some(value)
    } else {
        None
    };
    let Some(function) = STDLIB.map_fn(func) else {
        input.state.push_err(if eval {
            ParseError::UnsupportedMapEvaluation {
                span,
                name: func.to_string(),
            }
        } else {
            ParseError::UnsupportedMapFunction {
                span,
                name: func.to_string(),
            }
        });
        return Err(recorded());
    };
    Ok(Mapping {
        function: function.clone(),
        arg,
    })
}

/// `map (<calc_op> <number> | <func> ("(" <number> ")")?)`.
fn map_rule(input: &mut Input) -> PResult<Aggregate> {
    keyword(input, "map")?;
    cut_err(map_body)
        .context(StrContext::Label("map clause"))
        .parse_next(input)
}

fn map_body(input: &mut Input) -> PResult<Aggregate> {
    if let Some(op) = opt(calc_op).parse_next(input)? {
        let span = span_from(input, input.current_token_start());
        let value = cut_err(number_arg)
            .context(StrContext::Expected(StrContextValue::Description(
                "a number",
            )))
            .parse_next(input)?;
        let func = Function {
            module_path: vec![],
            name: FunctionId::new(op),
        };
        let mapping = map_function(input, &func, span, true)?;
        return Ok(Aggregate::Map(Mapping {
            arg: Some(value),
            ..mapping
        }));
    }
    let (func, span) = function_id(input)?;
    let mapping = map_function(input, &func, span, false)?;
    Ok(Aggregate::Map(mapping))
}

fn group_function(input: &mut Input) -> PResult<GroupFunction> {
    let (func, span) = function_id(input)?;
    let Some(function) = STDLIB.group_fn(&func) else {
        input.state.push_err(ParseError::UnsupportedGroupFunction {
            span,
            name: func.to_string(),
        });
        return Err(recorded());
    };
    Ok(function.clone())
}

/// `group (by <tags>)? using <func>`.
fn group_rule(input: &mut Input) -> PResult<Aggregate> {
    let start = token_start(input);
    keyword(input, "group")?;
    cut_err(move |i: &mut Input| group_body(i, start))
        .context(StrContext::Label("group clause"))
        .parse_next(input)
}

fn group_body(input: &mut Input, start: usize) -> PResult<Aggregate> {
    let tags = if opt(|i: &mut Input| keyword(i, "by"))
        .parse_next(input)?
        .is_some()
    {
        tags(input)?
    } else {
        Vec::new()
    };
    keyword(input, "using")?;
    let function = group_function(input)?;
    Ok(Aggregate::GroupBy(GroupBy {
        span: span_from(input, start),
        function,
        tags,
    }))
}

fn bucket_conversion(input: &mut Input) -> PResult<ConversionMethod> {
    let (name, span) = ident_spanned(input)?;
    match name.as_str() {
        "rate" => Ok(ConversionMethod::Rate),
        "increase" => Ok(ConversionMethod::Increase),
        other => {
            input.state.push_err(ParseError::UnsupportedBucketFunction {
                span,
                name: other.to_string(),
            });
            Err(recorded())
        }
    }
}

fn bucket_spec(input: &mut Input) -> PResult<BucketSpec> {
    if let Some(value) = opt(number_arg).parse_next(input)? {
        return Ok(BucketSpec::Percentile(value));
    }
    let (name, span) = ident_spanned(input)?;
    match name.as_str() {
        "count" => Ok(BucketSpec::Count),
        "avg" => Ok(BucketSpec::Avg),
        "sum" => Ok(BucketSpec::Sum),
        "min" => Ok(BucketSpec::Min),
        "max" => Ok(BucketSpec::Max),
        other => {
            input.state.push_err(ParseError::SyntaxError {
                span,
                label: "bucket specification".to_string(),
                message: format!("`{other}` is not a valid bucket spec"),
                suggestion: None,
            });
            Err(recorded())
        }
    }
}

fn bucket_specs(input: &mut Input) -> PResult<Vec<BucketSpec>> {
    let mut out = vec![bucket_spec(input)?];
    while opt(|i: &mut Input| symbol(i, ","))
        .parse_next(input)?
        .is_some()
    {
        out.push(bucket_spec(input)?);
    }
    Ok(out)
}

fn bucket_fn_call(input: &mut Input) -> PResult<(BucketType, Vec<BucketSpec>)> {
    let (name, span) = ident_spanned(input)?;
    let function = match name.as_str() {
        "histogram" => BucketType::Histogram,
        "interpolate_delta_histogram" => BucketType::InterpolateDeltaHistogram,
        "interpolate_cumulative_histogram" => {
            symbol(input, "(")?;
            let conversion = bucket_conversion(input)?;
            symbol(input, ",")?;
            let specs = bucket_specs(input)?;
            cut_err(|i: &mut Input| symbol(i, ")"))
                .context(StrContext::Expected(StrContextValue::CharLiteral(')')))
                .parse_next(input)?;
            return Ok((
                BucketType::InterpolateCumulativeHistogram(conversion),
                specs,
            ));
        }
        other => {
            input.state.push_err(ParseError::UnsupportedBucketFunction {
                span,
                name: other.to_string(),
            });
            return Err(recorded());
        }
    };
    symbol(input, "(")?;
    let specs = bucket_specs(input)?;
    cut_err(|i: &mut Input| symbol(i, ")"))
        .context(StrContext::Expected(StrContextValue::CharLiteral(')')))
        .parse_next(input)?;
    Ok((function, specs))
}

/// `bucket (by <tags>)? (to <reltime>)? using <bucket_fn_call>`.
fn bucket_rule(input: &mut Input) -> PResult<Aggregate> {
    let start = token_start(input);
    keyword(input, "bucket")?;
    cut_err(move |i: &mut Input| bucket_body(i, start))
        .context(StrContext::Label("bucket clause"))
        .parse_next(input)
}

fn bucket_body(input: &mut Input, start: usize) -> PResult<Aggregate> {
    let tags = if opt(|i: &mut Input| keyword(i, "by"))
        .parse_next(input)?
        .is_some()
    {
        tags(input)?
    } else {
        Vec::new()
    };
    let time = if opt(|i: &mut Input| keyword(i, "to"))
        .parse_next(input)?
        .is_some()
    {
        Some(param_relative_time(input)?)
    } else {
        None
    };
    keyword(input, "using")?;
    let (function, spec) = bucket_fn_call(input)?;
    Ok(Aggregate::Bucket(BucketBy {
        span: span_from(input, start),
        function,
        time,
        tags,
        spec,
    }))
}

/// `join <tags> from <metric_id> by <tags>` — recognised but unsupported.
fn join_rule(input: &mut Input) -> PResult<Aggregate> {
    let start = token_start(input);
    keyword(input, "join")?;
    input.state.push_err(ParseError::NotSupported {
        span: span_from(input, start),
        rule: UnsupportedRule::Join,
    });
    Err(recorded())
}

/// `replace <tag> (= <tag>)? (~ #s/…/…/)?` — recognised but unsupported.
fn replace_rule(input: &mut Input) -> PResult<Aggregate> {
    let start = token_start(input);
    keyword(input, "replace")?;
    cut_err(move |i: &mut Input| replace_body(i, start))
        .context(StrContext::Label("replace clause"))
        .parse_next(input)
}

fn replace_body(input: &mut Input, start: usize) -> PResult<Aggregate> {
    let _ = tag_name(input)?;
    if opt(|i: &mut Input| symbol(i, "~"))
        .parse_next(input)?
        .is_some()
    {
        regex_replace_literal(input)?;
    } else {
        symbol(input, "=")?;
        let _ = tag_name(input)?;
        if opt(|i: &mut Input| symbol(i, "~"))
            .parse_next(input)?
            .is_some()
        {
            regex_replace_literal(input)?;
        }
    }
    input.state.push_err(ParseError::NotSupported {
        span: span_from(input, start),
        rule: UnsupportedRule::Replace,
    });
    Err(recorded())
}

/// `as <metric_name>` (pipe form).
fn as_pipe(input: &mut Input) -> PResult<Aggregate> {
    let as_ = as_rename(input)?;
    Ok(Aggregate::As(as_))
}

/// `extend <tag> = <expr> (, <tag> = <expr>)*`.
fn extend_rule(input: &mut Input) -> PResult<Vec<TagExtend>> {
    keyword(input, "extend")?;
    cut_err(|i: &mut Input| {
        let mut out = vec![extend_expr(i)?];
        while opt(|i: &mut Input| symbol(i, ",")).parse_next(i)?.is_some() {
            out.push(extend_expr(i)?);
        }
        Ok(out)
    })
    .context(StrContext::Label("extend clause"))
    .parse_next(input)
}

fn extend_expr(input: &mut Input) -> PResult<TagExtend> {
    let tag = tag_name(input)?;
    symbol(input, "=")?;
    let value = expr(input)?;
    Ok(TagExtend { tag, value })
}

/// `sample <number>` (only valid right after the source, deduped).
fn sample_rule(input: &mut Input) -> PResult<f64> {
    keyword(input, "sample")?;
    cut_err(number_arg)
        .context(StrContext::Label("sample rate"))
        .parse_next(input)
}

// ── pipe-clause dispatch ────────────────────────────────────────

enum Clause {
    Filter(FilterOrIfDef),
    Aggregate(Aggregate),
    Extend(Vec<TagExtend>),
    Sample(f64),
}

fn pipe_clause(input: &mut Input) -> PResult<Clause> {
    alt((
        alt((
            sample_rule.map(Clause::Sample),
            filter_rule.map(Clause::Filter),
            ifdef_rule.map(Clause::Filter),
            map_rule.map(Clause::Aggregate),
            align_rule.map(Clause::Aggregate),
            group_rule.map(Clause::Aggregate),
        )),
        alt((
            bucket_rule.map(Clause::Aggregate),
            join_rule.map(Clause::Aggregate),
            replace_rule.map(Clause::Aggregate),
            as_pipe.map(Clause::Aggregate),
            extend_rule.map(Clause::Extend),
        )),
    ))
    .context(StrContext::Label("pipe clause"))
    .parse_next(input)
}

// ── param declarations (preamble) ───────────────────────────────

/// One of `string|int|float|bool|Dataset|Duration|Regex` (+ legacy `duration`),
/// returned with its source span.
fn terminal_type(input: &mut Input) -> PResult<(TerminalParamType, SourceSpan)> {
    let (name, span) = ident_spanned(input)?;
    let typ = match name.as_str() {
        "string" => TerminalParamType::Tag(TagType::String),
        "int" => TerminalParamType::Tag(TagType::Int),
        "float" => TerminalParamType::Tag(TagType::Float),
        "bool" => TerminalParamType::Tag(TagType::Bool),
        "Dataset" => TerminalParamType::Dataset,
        "Duration" => TerminalParamType::Duration,
        "Regex" => TerminalParamType::Regex,
        "duration" => {
            input.state.push_warning(span, WarningReason::OldDuration);
            TerminalParamType::Duration
        }
        other => {
            input.state.push_err(ParseError::SyntaxError {
                span,
                label: "invalid param type".to_string(),
                message: format!("`{other}` is not a valid param type"),
                suggestion: None,
            });
            return Err(recorded());
        }
    };
    Ok((typ, span))
}

fn param_type(input: &mut Input) -> PResult<ParamType> {
    if opt(|i: &mut Input| keyword(i, "Option"))
        .parse_next(input)?
        .is_some()
    {
        symbol(input, "<")?;
        let (inner, span) = terminal_type(input)?;
        cut_err(|i: &mut Input| symbol(i, ">"))
            .context(StrContext::Expected(StrContextValue::CharLiteral('>')))
            .parse_next(input)?;
        match inner {
            TerminalParamType::Duration | TerminalParamType::Dataset => {
                input.state.push_err(ParseError::SyntaxError {
                    span,
                    label: "invalid optional type".to_string(),
                    message: format!("`Option<{inner}>` is not allowed"),
                    suggestion: None,
                });
                Err(recorded())
            }
            other => Ok(ParamType::Optional(other)),
        }
    } else {
        terminal_type(input).map(|(typ, _)| ParamType::Terminal(typ))
    }
}

fn param_declaration(input: &mut Input) -> PResult<ParamDeclaration> {
    keyword(input, "param")?;
    let (name, span) = cut_err(param_ref)
        .context(StrContext::Label("param name"))
        .parse_next(input)?;
    cut_err(|i: &mut Input| symbol(i, ":"))
        .context(StrContext::Expected(StrContextValue::CharLiteral(':')))
        .parse_next(input)?;
    let typ = cut_err(param_type)
        .context(StrContext::Label("param type"))
        .parse_next(input)?;
    cut_err(|i: &mut Input| symbol(i, ";"))
        .context(StrContext::Expected(StrContextValue::CharLiteral(';')))
        .parse_next(input)?;
    Ok(ParamDeclaration { span, name, typ })
}

/// `set <ident> (= (<const>|<ident>))? ;`.
fn directive(input: &mut Input) -> PResult<(String, DirectiveValue)> {
    keyword(input, "set")?;
    cut_err(|i: &mut Input| {
        let name = tag_name(i)?;
        let value = if opt(|i: &mut Input| symbol(i, "=")).parse_next(i)?.is_some() {
            directive_value(i)?
        } else {
            DirectiveValue::None
        };
        cut_err(|i: &mut Input| symbol(i, ";"))
            .context(StrContext::Expected(StrContextValue::CharLiteral(';')))
            .parse_next(i)?;
        Ok((name, value))
    })
    .context(StrContext::Label("directive"))
    .parse_next(input)
}

fn directive_value(input: &mut Input) -> PResult<DirectiveValue> {
    let _ = trivia(input);
    if peek_char(input) == Some('"') {
        return string_raw(input).map(DirectiveValue::String);
    }
    if let Some(value) = opt(bool_lit).parse_next(input)? {
        return Ok(DirectiveValue::Bool(value));
    }
    if let Some(value) = opt(inf_lit).parse_next(input)? {
        return Ok(DirectiveValue::Float(value));
    }
    if let Some(value) = opt(number_lit).parse_next(input)? {
        return Ok(match value {
            TagValue::Int(v) => DirectiveValue::Int(v),
            TagValue::Float(v) => DirectiveValue::Float(v),
            _ => DirectiveValue::None,
        });
    }
    tag_name(input).map(DirectiveValue::Ident)
}

// ── orchestration & recovery ────────────────────────────────────

/// Parse a full `file` (`directive* param* query EOI`).
///
/// `system_params` are host-injected declarations (already validated for the
/// `__` prefix) merged ahead of inline declarations.
#[must_use]
pub fn parse_file(src: &str, system_params: Vec<ParamDeclaration>) -> ParseOutput {
    let diags = RefCell::new(Diags::default());
    let mut params = system_params;
    let mut directives = Directives::default();
    let empty_dir = Directives::default();

    let query = {
        let mut pre = Stateful {
            input: LocatingSlice::new(src),
            state: Ctx {
                params: &[],
                directives: &empty_dir,
                diags: &diags,
            },
        };
        parse_preamble(&mut pre, &mut params, &mut directives);
        let located = pre.input;
        let mut body = Stateful {
            input: located,
            state: Ctx {
                params: &params,
                directives: &directives,
                diags: &diags,
            },
        };
        parse_query(&mut body, false)
    };

    let diags = diags.into_inner();
    ParseOutput {
        query,
        errors: diags.errors,
        warnings: diags.warnings,
    }
}

/// Parse `directive*` then `param*`, appending declarations to `params` and
/// directives to `directives`.
fn parse_preamble(
    input: &mut Input,
    params: &mut Vec<ParamDeclaration>,
    directives: &mut Directives,
) {
    loop {
        let _ = trivia(input);
        let start = input.checkpoint();

        // directive?
        match directive(input) {
            Ok((name, value)) => {
                directives.insert(name, value);
                continue;
            }
            Err(ErrMode::Backtrack(_)) => input.reset(&start),
            Err(_) => {
                record_clause_error(input, "directive");
                resync_to(input, is_semicolon);
                let _ = lit(input, ";");
                continue;
            }
        }

        // param declaration?
        let before = input.state.error_count();
        match param_declaration(input) {
            Ok(decl) => register_param(input, params, decl),
            Err(ErrMode::Backtrack(_)) => {
                input.reset(&start);
                break;
            }
            Err(_) => {
                if input.state.error_count() == before {
                    record_syntax_error(input, "param declaration");
                }
                resync_to(input, is_semicolon);
                let _ = lit(input, ";");
            }
        }
    }
}

fn register_param(input: &mut Input, params: &mut Vec<ParamDeclaration>, decl: ParamDeclaration) {
    if decl.name.starts_with(SYSTEM_PARAM_PREFIX) {
        input.state.push_warning(
            decl.span,
            WarningReason::ParamUsingSystemPrefix {
                param: decl.name.clone(),
            },
        );
    } else if params.iter().any(|p| p.name == decl.name) {
        input.state.push_err(ParseError::ParamDefinedMultipleTimes {
            span: decl.span,
            param: decl.name.clone(),
        });
        return;
    }
    params.push(decl);
}

/// Parse a `query` node: a `compute_query` if it starts with `(`, else a
/// `simple_query`. `nested` is set inside a `compute` sub-query so the clause
/// loop and resync stop at `,`/`)` boundaries.
fn parse_query(input: &mut Input, nested: bool) -> Option<Query> {
    let _ = trivia(input);
    if peek_char(input) == Some('(') {
        parse_compute(input, nested)
    } else {
        parse_simple_query(input, nested)
    }
}

fn parse_simple_query(input: &mut Input, nested: bool) -> Option<Query> {
    let before = input.state.error_count();
    let Ok((source, as_)) = source(input) else {
        if input.state.error_count() == before {
            record_syntax_error(input, "metric source");
        }
        return None;
    };

    let mut sample = None;
    let mut filters = Vec::new();
    let mut aggregates = Vec::new();
    let mut extends = Vec::new();
    if let Some(as_) = as_ {
        aggregates.push(Aggregate::As(as_));
    }

    parse_clauses(
        input,
        nested,
        &mut sample,
        &mut filters,
        &mut aggregates,
        &mut extends,
    );

    Some(Query::Simple {
        sample,
        source,
        filters,
        aggregates,
        extends,
        directives: input.state.directives.clone(),
        params: input.state.params.to_vec(),
    })
}

fn parse_compute(input: &mut Input, nested: bool) -> Option<Query> {
    if symbol(input, "(").is_err() {
        record_syntax_error(input, "compute query");
        return None;
    }
    let left = Box::new(parse_query(input, true)?);
    if symbol(input, ",").is_err() {
        record_syntax_error(input, "compute query `,` separator");
        return None;
    }
    let right = Box::new(parse_query(input, true)?);
    let _ = opt(|i: &mut Input| symbol(i, ",")).parse_next(input);
    if symbol(input, ")").is_err() {
        record_syntax_error(input, "compute query `)`");
        return None;
    }

    // compute_rule: `| compute <metric_name> using <compute_fn>`
    let _ = trivia(input);
    if literal::<_, _, ContextError>("|")
        .parse_next(input)
        .is_err()
    {
        record_syntax_error(input, "`|` before compute");
        return None;
    }
    if keyword(input, "compute").is_err() {
        record_syntax_error(input, "`compute` keyword");
        return None;
    }
    let name = metric_name(input).ok()?;
    if keyword(input, "using").is_err() {
        record_syntax_error(input, "`using` keyword");
        return None;
    }
    let op = compute_function(input).ok()?;

    let mut sample = None;
    let mut filters = Vec::new();
    let mut aggregates = Vec::new();
    let mut extends = Vec::new();
    parse_clauses(
        input,
        nested,
        &mut sample,
        &mut filters,
        &mut aggregates,
        &mut extends,
    );

    Some(Query::Compute {
        left,
        right,
        name,
        op,
        aggregates,
        extends,
        directives: input.state.directives.clone(),
        params: input.state.params.to_vec(),
    })
}

fn compute_function(input: &mut Input) -> PResult<ComputeFunction> {
    let (func, span) = if let Some(op) = opt(calc_op).parse_next(input)? {
        (
            Function {
                module_path: vec![],
                name: FunctionId::new(op),
            },
            span_from(input, input.current_token_start()),
        )
    } else {
        function_id(input)?
    };
    let Some(function) = STDLIB.compute_fn(&func) else {
        input
            .state
            .push_err(ParseError::UnsupportedComputeFunction {
                span,
                name: func.to_string(),
            });
        return Err(recorded());
    };
    Ok(function.clone())
}

/// Parse `(| clause)*` with pipe-boundary recovery, appending to the
/// destination collections. Stops at EOF (and, when `nested`, at `,`/`)`).
fn parse_clauses(
    input: &mut Input,
    nested: bool,
    sample: &mut Option<f64>,
    filters: &mut Vec<FilterOrIfDef>,
    aggregates: &mut Vec<Aggregate>,
    extends: &mut Vec<TagExtend>,
) {
    loop {
        let _ = trivia(input);
        if at_eof(input) {
            break;
        }
        if nested && matches!(peek_char(input), Some(',' | ')')) {
            break;
        }

        let pipe_cp = input.checkpoint();
        if literal::<_, _, ContextError>("|")
            .parse_next(input)
            .is_err()
        {
            input.reset(&pipe_cp);
            record_syntax_error(input, "expected `|`");
            resync(input, nested);
            continue;
        }

        let before = input.state.error_count();
        match pipe_clause(input) {
            Ok(Clause::Filter(filter)) => filters.push(filter),
            Ok(Clause::Aggregate(agg)) => aggregates.push(agg),
            Ok(Clause::Extend(items)) => extends.extend(items),
            Ok(Clause::Sample(value)) => {
                if sample.is_none() {
                    *sample = Some(value);
                }
            }
            Err(err) => {
                if input.state.error_count() == before {
                    record_context_error(input, &err);
                }
                resync(input, nested);
            }
        }
    }
}

fn is_pipe(c: char) -> bool {
    c == '|'
}

fn is_pipe_or_boundary(c: char) -> bool {
    matches!(c, '|' | ',' | ')')
}

fn is_semicolon(c: char) -> bool {
    c == ';'
}

fn resync(input: &mut Input, nested: bool) {
    if nested {
        resync_to(input, is_pipe_or_boundary);
    } else {
        resync_to(input, is_pipe);
    }
}

/// Advance to the next top-level character matching `stop` (or EOF), skipping
/// strings, regexes, backtick idents and comments so a delimiter inside them is
/// not mistaken for a boundary. This is the manual "resync at `|`" recovery.
fn resync_to(input: &mut Input, stop: fn(char) -> bool) {
    loop {
        let _ = trivia(input);
        if at_eof(input) {
            return;
        }
        if let Some(c) = peek_char(input)
            && stop(c)
        {
            return;
        }
        let _ = alt((
            skip_delimited("\"", '"'),
            skip_delimited("#/", '/'),
            skip_delimited("`", '`'),
            any.void(),
        ))
        .parse_next(input);
    }
}

/// Skip a `\`-escaping delimited literal opened by `open` and closed by `close`.
fn skip_delimited<'s>(
    open: &'static str,
    close: char,
) -> impl FnMut(&mut Input<'s>) -> PResult<()> {
    move |input: &mut Input<'s>| {
        literal(open).parse_next(input)?;
        let _: () = repeat(
            0..,
            alt((
                preceded(literal("\\"), any).void(),
                none_of([close, '\\']).void(),
            )),
        )
        .parse_next(input)?;
        opt(literal(close.to_string().as_str()))
            .void()
            .parse_next(input)
    }
}

fn record_clause_error(input: &mut Input, label: &str) {
    let at = input.current_token_start();
    input.state.push_err(ParseError::SyntaxError {
        span: SourceSpan::new(at.into(), 1),
        label: label.to_string(),
        message: format!("error in {label}"),
        suggestion: None,
    });
}

fn record_syntax_error(input: &mut Input, label: &str) {
    let at = input.current_token_start();
    input.state.push_err(ParseError::SyntaxError {
        span: SourceSpan::new(at.into(), 1),
        label: label.to_string(),
        message: format!("unexpected input while parsing {label}"),
        suggestion: None,
    });
}

/// Map a `winnow` [`ContextError`] into the repo's miette [`ParseError`],
/// using the current input offset for the span and the context stack for the
/// label/message.
fn record_context_error(input: &mut Input, err: &ErrMode<ContextError>) {
    let at = input.current_token_start();
    let ctx = match err {
        ErrMode::Backtrack(c) | ErrMode::Cut(c) => Some(c),
        ErrMode::Incomplete(_) => None,
    };

    let mut expected = Vec::new();
    let mut labels = Vec::new();
    if let Some(ctx) = ctx {
        for item in ctx.context() {
            match item {
                StrContext::Expected(value) => expected.push(value.to_string()),
                StrContext::Label(name) => labels.push((*name).to_string()),
                _ => {}
            }
        }
    }

    let label = if !expected.is_empty() {
        format!("expected {}", expected.join(" or "))
    } else if let Some(first) = labels.first() {
        format!("invalid {first}")
    } else {
        "unexpected token".to_string()
    };
    let message = labels
        .first()
        .map_or_else(|| "syntax error".to_string(), |l| format!("error in {l}"));

    input.state.push_err(ParseError::SyntaxError {
        span: SourceSpan::new(at.into(), 1),
        label,
        message,
        suggestion: None,
    });
}

// ── external `param_value` entry point ──────────────────────────

/// Parse a host-provided `param` value string into a typed [`ParamValue`],
/// directed by the param's declared type. The pest-free replacement for the
/// former `parser::parse_param_value` + `Rule::param_value` entry point.
pub fn parse_param_value(
    param: &ParamDeclaration,
    value: &str,
) -> Result<ParamValue, ParseParamError> {
    let diags = RefCell::new(Diags::default());
    let empty_dir = Directives::default();
    let mut input = Stateful {
        input: LocatingSlice::new(value),
        state: Ctx {
            params: &[],
            directives: &empty_dir,
            diags: &diags,
        },
    };

    let mismatch = |found: &'static str| ParseParamError::TypeMismatch {
        declared_typ: param.typ,
        found,
    };

    match param.typ() {
        TerminalParamType::Dataset => ident_name(&mut input)
            .map(|name| ParamValue::Dataset(Dataset::new(name)))
            .map_err(|_| mismatch("dataset name")),
        TerminalParamType::Duration => relative_time(&mut input)
            .map(ParamValue::Duration)
            .map_err(|_| mismatch("duration")),
        TerminalParamType::Regex => parse_regex_param(value, param),
        TerminalParamType::Tag(TagType::String) => string_raw(&mut input)
            .map(ParamValue::String)
            .map_err(|_| mismatch("string")),
        TerminalParamType::Tag(TagType::Int) => value
            .trim()
            .parse::<i64>()
            .map(ParamValue::Int)
            .map_err(|_| mismatch("integer")),
        TerminalParamType::Tag(TagType::Float) => match value.trim() {
            "inf" | "+inf" => Ok(ParamValue::Float(f64::INFINITY)),
            "-inf" => Ok(ParamValue::Float(f64::NEG_INFINITY)),
            other => other
                .parse::<f64>()
                .map(ParamValue::Float)
                .map_err(ParseParamError::ParseFloat),
        },
        TerminalParamType::Tag(TagType::Bool) => value
            .trim()
            .parse::<bool>()
            .map(ParamValue::Bool)
            .map_err(ParseParamError::ParseBool),
        TerminalParamType::Tag(TagType::Null) => Err(ParseParamError::NoneParam),
    }
}

fn parse_regex_param(value: &str, param: &ParamDeclaration) -> Result<ParamValue, ParseParamError> {
    let trimmed = value.trim();
    if !trimmed.starts_with("#/") || !trimmed.ends_with('/') || trimmed.len() < 3 {
        return Err(ParseParamError::TypeMismatch {
            declared_typ: param.typ,
            found: "regex (#/…/)",
        });
    }
    let pattern = unescape_and_trim(&trimmed[1..], '/');
    EncodableRegex::new(&pattern)
        .map(ParamValue::Regex)
        .map_err(|err| ParseParamError::Parse(ParseError::from(err)))
}

#[cfg(test)]
mod tests;
