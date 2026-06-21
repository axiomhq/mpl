//! PHASE B — CST FEASIBILITY SPIKE (winnow → rowan).
//!
//! This file is an **experiment**, not production code. It answers one
//! question: can `winnow` build a *lossless, position-addressable* `rowan`
//! concrete-syntax tree (CST) as cleanly as RW's hand-written recursive-descent
//! parser (see `depest-pi-rw/src/cst/parser.rs`)?
//!
//! It is deliberately a single, self-contained integration-test crate so it
//! **cannot affect the production build**: it is only compiled by `cargo test`,
//! it touches no production module (the `wparser` AST parser is untouched), and
//! `rowan` is a dev-dependency. Nothing here is lowered — the CST is the whole
//! deliverable.
//!
//! ## Representative slice (everything that matters, nothing that doesn't)
//! * a metric source `ds:metric`
//! * one pipe clause `| where <ident> (== | != | < | …) <number | string>`
//! * a string literal with a `${ <ident> }` interpolation (the recursive
//!   construct that broke `logos` in RW)
//! * trivia: leading/trailing whitespace and `// line comments`
//!
//! ## Architecture (the honest shape winnow forces)
//! The `GreenNodeBuilder` is threaded through the combinators as `winnow`
//! [`Stateful`] state (alongside a [`RefCell`] error sink), mirroring how the
//! production `wparser` threads its `Ctx`. The crucial constraint this exposes:
//! **`winnow`'s backtracking combinators (`alt`/`opt`) backtrack the input
//! position but NOT side-effects on the builder.** So the grammar is written
//! LL-style — decide with `peek`/lookahead *before* emitting, never emit on a
//! branch that may be abandoned. In practice this means re-implementing
//! recursive descent *inside* winnow; the combinators buy span tracking
//! (`LocatingSlice`) and tidy leaf lexers, little structural leverage. See
//! SPIKE.md for the full ledger and verdict.

use std::cell::RefCell;
use std::ops::Range;

use rowan::{GreenNode, GreenNodeBuilder, Language, NodeOrToken};
use winnow::{
    Parser,
    ascii::{digit1, multispace1, till_line_ending},
    combinator::{alt, eof, not, opt, peek, preceded, repeat},
    error::ContextError,
    stream::{LocatingSlice, Location, Stateful, Stream},
    token::{any, literal, one_of, take_while},
};

// ── syntax kinds ─────────────────────────────────────────────────
//
// Mirrors RW's `SyntaxKind` shape (screaming-case, `#[repr(u16)]`, trivia →
// tokens, semantic relabels assigned by the parser) but pared to the slice.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
// Screaming-case mirrors RW's `SyntaxKind` and the rust-analyzer/rowan convention.
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
enum SyntaxKind {
    // ── trivia tokens ──
    WHITESPACE,
    COMMENT,
    // ── literal / identifier tokens ──
    IDENT,
    INT,
    FLOAT,
    // ── string interior tokens ──
    STRING_FRAGMENT,
    DOLLAR_BRACE,
    R_BRACE,
    // ── punctuation tokens ──
    COLON,
    PIPE,
    // ── parser-assigned relabels ──
    KEYWORD,
    CMP_OP,
    ERROR,
    // ── interior nodes ──
    ROOT,
    QUERY,
    SOURCE,
    METRIC_ID,
    DATASET,
    METRIC_NAME,
    FILTER_RULE,
    FILTER_ATOM,
    VALUE,
    NUMBER,
    STRING,
    EXPR,
    ERROR_NODE,
}

impl SyntaxKind {
    fn raw(self) -> rowan::SyntaxKind {
        rowan::SyntaxKind(self as u16)
    }

    fn from_u16(raw: u16) -> Self {
        assert!(
            raw <= SyntaxKind::ERROR_NODE as u16,
            "raw syntax kind out of range"
        );
        // SAFETY: `SyntaxKind` is `#[repr(u16)]` with contiguous discriminants
        // `0..=ERROR_NODE`; the bounds check guarantees a valid variant and
        // rowan only round-trips kinds we produced.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw) }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum MplLang {}

impl Language for MplLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        SyntaxKind::from_u16(raw.0)
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.raw()
    }
}

type SyntaxNode = rowan::SyntaxNode<MplLang>;

// ── parse result ─────────────────────────────────────────────────

/// A recovery diagnostic with the byte range it applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SyntaxError {
    message: String,
    range: Range<usize>,
}

/// The result of [`parse`]: a green tree plus recovery diagnostics. Never fails.
struct Parse {
    green: GreenNode,
    errors: Vec<SyntaxError>,
}

impl Parse {
    fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green.clone())
    }

    fn errors(&self) -> &[SyntaxError] {
        &self.errors
    }
}

// ── winnow stream + threaded builder ─────────────────────────────

/// Parser state threaded through every combinator. `Copy` so it costs nothing
/// to pass around, exactly like `wparser::grammar::Ctx`. Interior mutability
/// (`RefCell`) lets combinators emit into the builder / push errors without a
/// `&mut` plumbing nightmare across `alt`/`repeat` closures.
#[derive(Clone, Copy)]
struct State<'a> {
    src: &'a str,
    builder: &'a RefCell<GreenNodeBuilder<'static>>,
    errors: &'a RefCell<Vec<SyntaxError>>,
}

// `Stateful<I, S>` only implements `winnow::Stream` when `S: Debug`; the
// builder/error refs have nothing useful to print, so this is opaque.
impl std::fmt::Debug for State<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State").finish_non_exhaustive()
    }
}

type Input<'s> = Stateful<LocatingSlice<&'s str>, State<'s>>;
type PResult<T> = winnow::Result<T, ContextError>;

/// Parse `src` into a lossless [`Parse`]. Total: never panics, never fails.
fn parse(src: &str) -> Parse {
    let builder = RefCell::new(GreenNodeBuilder::new());
    let errors = RefCell::new(Vec::new());
    {
        let mut input: Input = Stateful {
            input: LocatingSlice::new(src),
            state: State {
                src,
                builder: &builder,
                errors: &errors,
            },
        };
        file(&mut input);
    }
    Parse {
        green: builder.into_inner().finish(),
        errors: errors.into_inner(),
    }
}

// ── builder helpers ──────────────────────────────────────────────

fn emit(input: &mut Input, kind: SyntaxKind, text: &str) {
    input.state.builder.borrow_mut().token(kind.raw(), text);
}

/// Emit the source slice `lo..hi` as one `kind` token (string fragments use
/// this so the boundary quotes glue onto their adjacent fragment, like RW).
fn emit_slice(input: &mut Input, kind: SyntaxKind, lo: usize, hi: usize) {
    let src = input.state.src;
    let text = &src[lo..hi];
    emit(input, kind, text);
}

fn start(input: &mut Input, kind: SyntaxKind) {
    input.state.builder.borrow_mut().start_node(kind.raw());
}

fn finish(input: &mut Input) {
    input.state.builder.borrow_mut().finish_node();
}

fn checkpoint(input: &Input) -> rowan::Checkpoint {
    input.state.builder.borrow().checkpoint()
}

/// Retroactively open a node at `cp` (used to label a pipe clause only after
/// peeking past the `|` to learn which rule it is).
fn wrap(input: &mut Input, cp: rowan::Checkpoint, kind: SyntaxKind) {
    input
        .state
        .builder
        .borrow_mut()
        .start_node_at(cp, kind.raw());
}

fn error(input: &mut Input, message: &str) {
    let at = current_off(input);
    input.state.errors.borrow_mut().push(SyntaxError {
        message: message.to_string(),
        range: at..at,
    });
}

fn error_range(input: &mut Input, message: &str, lo: usize, hi: usize) {
    input.state.errors.borrow_mut().push(SyntaxError {
        message: message.to_string(),
        range: lo..hi,
    });
}

fn current_off(input: &Input) -> usize {
    input.current_token_start()
}

// ── lookahead / leaf lexers (no side-effects, safe in `alt`/`peek`) ──

fn at_eof(input: &mut Input) -> bool {
    eof::<_, ContextError>.parse_next(input).is_ok()
}

fn peek_is(input: &mut Input, lit: &'static str) -> bool {
    let r: PResult<&str> = peek(literal(lit)).parse_next(input);
    r.is_ok()
}

/// A plain identifier core: `(alpha|_)(alnum|_)*`. Atomic (no consume on fail).
fn word<'s>(input: &mut Input<'s>) -> PResult<&'s str> {
    (
        one_of(|c: char| c.is_ascii_alphabetic() || c == '_'),
        take_while(0.., |c: char| c.is_ascii_alphanumeric() || c == '_'),
    )
        .take()
        .parse_next(input)
}

/// `123` or `1.5`. Atomic (no consume on fail).
fn number<'s>(input: &mut Input<'s>) -> PResult<&'s str> {
    (digit1, opt(preceded(literal("."), digit1)))
        .take()
        .parse_next(input)
}

/// Whether the next non-trivia word equals `kw`. Caller must have eaten trivia.
/// Hand-rolled `peek` (checkpoint + reset) because a lifetime-generic parser
/// fn like `word` does not satisfy `peek`'s `impl Parser` bound by method call.
fn at_kw(input: &mut Input, kw: &str) -> bool {
    let start = input.checkpoint();
    let w = word(input);
    input.reset(&start);
    matches!(w, Ok(s) if s == kw)
}

fn at_cmp(input: &mut Input) -> bool {
    ["==", "!=", "<=", ">=", "<", ">"]
        .into_iter()
        .any(|op| peek_is(input, op))
}

// ── side-effecting bumpers (consume + emit; only on definite match) ──

/// Consume `lit` if present and emit it as `kind`; report whether it matched.
/// `literal` is atomic so this is safe to *try* without prior peeking.
fn bump_lit(input: &mut Input, lit: &'static str, kind: SyntaxKind) -> bool {
    let r: PResult<&str> = literal(lit).parse_next(input);
    match r {
        Ok(t) => {
            emit(input, kind, t);
            true
        }
        Err(_) => false,
    }
}

/// Consume an identifier if present and emit it as `kind`; report the match.
fn bump_word(input: &mut Input, kind: SyntaxKind) -> bool {
    match word(input) {
        Ok(t) => {
            emit(input, kind, t);
            true
        }
        Err(_) => false,
    }
}

/// Consume one comparison operator, emitting it as `CMP_OP`.
fn bump_cmp(input: &mut Input) {
    for op in ["==", "!=", "<=", ">=", "<", ">"] {
        if bump_lit(input, op, SyntaxKind::CMP_OP) {
            return;
        }
    }
}

/// Consume one char as an `ERROR` token (lossless recovery fill).
fn bump_any(input: &mut Input) {
    let r: PResult<&str> = any.take().parse_next(input);
    if let Ok(t) = r {
        emit(input, SyntaxKind::ERROR, t);
    }
}

/// Emit whitespace runs and `// line comments` as trivia tokens into the
/// currently-open node. Trivia emission is always safe to do eagerly: which
/// node trivia attaches to never affects losslessness.
fn trivia(input: &mut Input) {
    loop {
        let ws: PResult<&str> = multispace1.take().parse_next(input);
        if let Ok(t) = ws {
            emit(input, SyntaxKind::WHITESPACE, t);
            continue;
        }
        let cm: PResult<&str> = (literal("//"), till_line_ending).take().parse_next(input);
        if let Ok(t) = cm {
            emit(input, SyntaxKind::COMMENT, t);
            continue;
        }
        break;
    }
}

// ── grammar ──────────────────────────────────────────────────────

/// `file = trivia query? trivia error_node?`
fn file(input: &mut Input) {
    use SyntaxKind::{ERROR_NODE, ROOT};
    start(input, ROOT);
    trivia(input);
    if !at_eof(input) {
        query(input);
    }
    trivia(input);
    if !at_eof(input) {
        // Anything left over is unparseable: keep every byte in an error node.
        error(input, "unexpected trailing input");
        start(input, ERROR_NODE);
        loop {
            trivia(input);
            if at_eof(input) {
                break;
            }
            bump_any(input);
        }
        finish(input);
    }
    finish(input);
}

/// `query = source (pipe_clause)*`
fn query(input: &mut Input) {
    start(input, SyntaxKind::QUERY);
    source(input);
    loop {
        trivia(input);
        if peek_is(input, "|") {
            pipe_clause(input);
        } else {
            break;
        }
    }
    finish(input);
}

/// `source = metric_id`
fn source(input: &mut Input) {
    start(input, SyntaxKind::SOURCE);
    metric_id(input);
    finish(input);
}

/// `metric_id = dataset ':' metric_name`
fn metric_id(input: &mut Input) {
    use SyntaxKind::{COLON, DATASET, IDENT, METRIC_ID, METRIC_NAME};
    start(input, METRIC_ID);

    start(input, DATASET);
    trivia(input);
    if !bump_word(input, IDENT) {
        error(input, "expected a dataset name");
    }
    finish(input);

    trivia(input);
    if bump_lit(input, ":", COLON) {
        start(input, METRIC_NAME);
        trivia(input);
        if !bump_word(input, IDENT) {
            error(input, "expected a metric name");
        }
        finish(input);
    } else {
        error(input, "expected `:` and a metric name after the dataset");
    }

    finish(input);
}

/// Dispatch a `| …` pipe clause. The clause node kind is only known after the
/// keyword, so we checkpoint before `|` and `start_node_at` once decided.
fn pipe_clause(input: &mut Input) {
    use SyntaxKind::{ERROR_NODE, FILTER_RULE, KEYWORD, PIPE};
    let cp = checkpoint(input);
    trivia(input);
    bump_lit(input, "|", PIPE);
    trivia(input);
    if at_kw(input, "where") || at_kw(input, "filter") {
        wrap(input, cp, FILTER_RULE);
        bump_word(input, KEYWORD); // where / filter
        filter_atom(input);
        finish(input);
    } else {
        // Out-of-slice / unknown clause: keep its bytes in an error node and
        // resync at the next pipe (winnow has no built-in recovery).
        wrap(input, cp, ERROR_NODE);
        error(input, "unsupported pipe rule");
        loop {
            trivia(input);
            if at_eof(input) || peek_is(input, "|") {
                break;
            }
            bump_any(input);
        }
        finish(input);
    }
}

/// `filter_atom = ident cmp value`
fn filter_atom(input: &mut Input) {
    use SyntaxKind::{FILTER_ATOM, IDENT};
    start(input, FILTER_ATOM);
    trivia(input);
    if !bump_word(input, IDENT) {
        error(input, "expected a tag name");
    }
    trivia(input);
    if at_cmp(input) {
        bump_cmp(input);
        value(input);
    } else {
        error(input, "expected a comparison operator");
    }
    finish(input);
}

/// `value = number | string`
fn value(input: &mut Input) {
    use SyntaxKind::{FLOAT, INT, NUMBER, VALUE};
    trivia(input);
    start(input, VALUE);
    if peek_is(input, "\"") {
        string(input);
    } else {
        match number(input) {
            Ok(t) => {
                start(input, NUMBER);
                let kind = if t.contains('.') { FLOAT } else { INT };
                emit(input, kind, t);
                finish(input);
            }
            Err(_) => error(input, "expected a number or string value"),
        }
    }
    finish(input);
}

/// A double-quoted string, descending into `${ … }` interpolation so the
/// interior is real nodes/tokens, not one opaque `STRING` blob. Tolerates an
/// unterminated literal (mid-edit), diagnosing it over its full extent.
///
/// Caller guarantees the next char is `"`.
fn string(input: &mut Input) {
    use SyntaxKind::{DOLLAR_BRACE, R_BRACE, STRING, STRING_FRAGMENT};
    start(input, STRING);
    let str_start = current_off(input);
    // Opening quote begins the first literal run.
    let _: PResult<&str> = literal("\"").parse_next(input);
    let mut frag_lo = str_start;
    let mut terminated = false;

    loop {
        // Escape-aware literal run up to `${`, the closing `"`, or EOF.
        let _ = string_run(input);
        let here = current_off(input);

        let close: PResult<&str> = literal("\"").parse_next(input);
        if close.is_ok() {
            // Closing quote glues onto the trailing fragment.
            emit_slice(input, STRING_FRAGMENT, frag_lo, current_off(input));
            terminated = true;
            break;
        }

        let dollar: PResult<&str> = literal("${").parse_next(input);
        if dollar.is_ok() {
            if here > frag_lo {
                emit_slice(input, STRING_FRAGMENT, frag_lo, here);
            }
            emit_slice(input, DOLLAR_BRACE, here, current_off(input));
            expr(input); // recurse: the embedded expression is a real EXPR node
            trivia(input);
            let brace_lo = current_off(input);
            let brace: PResult<&str> = literal("}").parse_next(input);
            if brace.is_ok() {
                emit_slice(input, R_BRACE, brace_lo, current_off(input));
            } else {
                error(input, "expected `}` to close `${`");
            }
            frag_lo = current_off(input);
            continue;
        }

        // EOF without a closing quote: flush the trailing run (if any).
        if here > frag_lo {
            emit_slice(input, STRING_FRAGMENT, frag_lo, here);
        }
        break;
    }

    if !terminated {
        let end = current_off(input);
        error_range(input, "unterminated string", str_start, end);
    }
    finish(input);
}

/// Consume one escape-aware literal run inside a string, stopping before `${`,
/// the closing `"`, or EOF. Mirrors `wparser::lex::string_run_body`. Never
/// fails (a lone `$` not followed by `{` is literal text).
fn string_run(input: &mut Input) -> PResult<()> {
    repeat::<_, _, (), _, _>(
        0..,
        alt((
            preceded(literal("\\"), any).void(),
            (not(literal("\"")), not(literal("${")), any).void(),
        )),
    )
    .parse_next(input)
}

/// `expr = string | ident | number` — the body of a `${ … }` interpolation.
fn expr(input: &mut Input) {
    use SyntaxKind::{EXPR, FLOAT, IDENT, INT};
    start(input, EXPR);
    trivia(input);
    if peek_is(input, "\"") {
        string(input); // nested string interpolation
    } else if bump_word(input, IDENT) {
        // plain identifier interpolation, e.g. `${ name }`
    } else {
        match number(input) {
            Ok(t) => {
                let kind = if t.contains('.') { FLOAT } else { INT };
                emit(input, kind, t);
            }
            Err(_) => error(input, "expected an interpolation expression"),
        }
    }
    finish(input);
}

// ── tests ────────────────────────────────────────────────────────

/// Render the tree as `KIND "text"` lines (snapshot-style debugging aid).
fn dump(input: &str) -> String {
    fn go(node: &SyntaxNode, depth: usize, out: &mut String) {
        use std::fmt::Write as _;
        for child in node.children_with_tokens() {
            match child {
                NodeOrToken::Node(n) => {
                    let _ = writeln!(out, "{}{:?}", "  ".repeat(depth), n.kind());
                    go(&n, depth + 1, out);
                }
                NodeOrToken::Token(t) => {
                    let _ = writeln!(out, "{}{:?} {:?}", "  ".repeat(depth), t.kind(), t.text());
                }
            }
        }
    }
    let mut out = String::new();
    go(&parse(input).syntax(), 0, &mut out);
    out
}

/// PROPERTY 1 — byte-for-byte LOSSLESS round-trip, including trivia,
/// interpolation interiors, AND error-recovery fill.
#[test]
fn property1_lossless_roundtrip_preserves_every_byte() {
    let inputs = [
        "",
        "   ",
        "ds:cpu",
        "ds:cpu | where region == \"eu\"",
        "ds:cpu | where region == 42",
        // leading + trailing whitespace and line comments
        "// header\nds:cpu // trailing\n| where region == 1.5\n",
        // interpolation in the middle, adjacent text, escaped (non-)interp
        "ds:cpu | where tag == \"Hello ${ name }!\"",
        "ds:cpu | where tag == \"price \\${ 5 }\"",
        "ds:cpu | where tag == \"${ x }\"",
        // recovery cases must ALSO round-trip
        "ds:metric | where ",           // incomplete clause
        "ds:cpu | where x == \"a ${ b", // unterminated string
        "ds:cpu | frobnicate 5",        // unknown pipe rule
        "{{{}}}",
        "|||",
        "(",
        "ds:",
    ];
    for input in inputs {
        let parsed = parse(input);
        assert_eq!(
            parsed.syntax().text(),
            input,
            "roundtrip failed for {input:?}\n--- tree ---\n{}",
            dump(input)
        );
    }
}

/// PROPERTY 2 — the `${ ident }` interior is addressable as its own
/// nodes/tokens, not one opaque STRING blob.
#[test]
fn property2_interpolation_interior_is_addressable() {
    let input = "ds:cpu | where tag == \"Hello ${ name }!\"";
    let parsed = parse(input);
    assert_eq!(parsed.syntax().text(), input);

    let string = parsed
        .syntax()
        .descendants()
        .find(|n| n.kind() == SyntaxKind::STRING)
        .expect("a STRING node");

    // Not one opaque token: the literal is structured.
    let element_count = string.children_with_tokens().count();
    assert!(
        element_count > 1,
        "STRING must be structured, got {element_count} element(s)"
    );

    // The `${` delimiter is its own token.
    assert!(
        string
            .children_with_tokens()
            .any(|e| matches!(e, NodeOrToken::Token(t) if t.kind() == SyntaxKind::DOLLAR_BRACE)),
        "the `${{` delimiter is addressable"
    );

    // The embedded expression is a real EXPR subtree carrying `name`.
    let expr = string
        .children()
        .find(|n| n.kind() == SyntaxKind::EXPR)
        .expect("an embedded EXPR node");
    assert!(
        expr.descendants_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::IDENT && t.text() == "name"),
        "the interpolation expr holds the `name` ident"
    );

    // Boundary fragments carry the surrounding quotes (lossless prerequisite).
    let frags: Vec<String> = string
        .children_with_tokens()
        .filter_map(NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::STRING_FRAGMENT)
        .map(|t| t.text().to_string())
        .collect();
    assert_eq!(frags, vec!["\"Hello ".to_string(), "!\"".to_string()]);
}

/// PROPERTY 3a — an incomplete clause `ds:metric | where ` still produces a
/// tree (with an error marker) and round-trips losslessly.
#[test]
fn property3a_recovers_from_incomplete_clause() {
    let input = "ds:metric | where ";
    let parsed = parse(input);
    assert_eq!(parsed.syntax().text(), input);
    assert!(
        !parsed.errors().is_empty(),
        "expected a recovery diagnostic"
    );
    // The recognised prefix is still structured.
    assert!(
        parsed
            .syntax()
            .descendants()
            .any(|n| n.kind() == SyntaxKind::FILTER_RULE),
        "the `| where` prefix is still a FILTER_RULE"
    );
}

/// PROPERTY 3b — an unterminated string `"a ${ b` still produces a tree with
/// its interior structured, is diagnosed over its full extent, and round-trips.
#[test]
fn property3b_recovers_from_unterminated_string() {
    let input = "ds:cpu | where x == \"a ${ b";
    let parsed = parse(input);

    // Lossless even mid-edit.
    assert_eq!(parsed.syntax().text(), input);

    // Diagnosed as unterminated over the whole literal (to EOF).
    assert!(
        parsed
            .errors()
            .iter()
            .any(|e| e.message == "unterminated string" && e.range.end == input.len()),
        "unterminated string diagnosed to EOF; got {:?}",
        parsed.errors()
    );

    // Interior is still structured: STRING → fragment + `${` + EXPR(IDENT b).
    let string = parsed
        .syntax()
        .descendants()
        .find(|n| n.kind() == SyntaxKind::STRING)
        .expect("a STRING node even for an unterminated literal");
    let expr = string
        .children()
        .find(|n| n.kind() == SyntaxKind::EXPR)
        .expect("an embedded EXPR node even mid-edit");
    assert!(
        expr.descendants_with_tokens()
            .filter_map(NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::IDENT && t.text() == "b"),
        "the in-progress `b` ident is preserved"
    );
}

/// Sanity: the slice structure is shaped as intended (positions are addressable
/// by node kind, the basis for editor features / a formatter).
#[test]
fn slice_structure_is_shaped_correctly() {
    let tree = dump("ds:cpu | where region == 42");
    for needle in [
        "QUERY",
        "SOURCE",
        "METRIC_ID",
        "DATASET",
        "METRIC_NAME",
        "FILTER_RULE",
        "FILTER_ATOM",
        "VALUE",
        "NUMBER",
        "KEYWORD \"where\"",
        "CMP_OP \"==\"",
        "INT \"42\"",
    ] {
        assert!(tree.contains(needle), "missing {needle:?} in:\n{tree}");
    }
}

/// An unknown pipe rule is preserved as an `ERROR_NODE`, not dropped.
#[test]
fn unknown_pipe_becomes_error_node() {
    let input = "ds:cpu | frobnicate 5";
    let parsed = parse(input);
    assert_eq!(parsed.syntax().text(), input);
    assert!(
        parsed
            .syntax()
            .descendants()
            .any(|n| n.kind() == SyntaxKind::ERROR_NODE),
        "unknown pipe kept as ERROR_NODE"
    );
}
