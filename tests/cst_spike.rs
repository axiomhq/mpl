//! PHASE B — CST FEASIBILITY SPIKE (chumsky → rowan).
//!
//! This is a *self-contained experiment*, gated as an integration test so it
//! never enters the library / wasm build. It answers ONE question: can
//! `chumsky` produce a lossless, position-addressable `rowan` CST as cleanly as
//! RW's hand-written recursive-descent parser
//! (`/Users/heinzgies/Projects/depest-pi-rw/src/cst/`)?
//!
//! Scope is a *representative slice*, not the whole grammar:
//!   (a) a metric source `ds:metric`
//!   (b) one pipe clause `| where <ident> == <number|string>`
//!   (c) a string literal with a `${ <ident> }` interpolation (the recursive
//!       construct that forced RW to hand-roll a byte scanner around `logos`)
//!   (d) trivia: leading/trailing whitespace and `// line comments`
//!
//! Architecture (mirrors rust-analyzer / RW conceptually, but driven by
//! chumsky combinators instead of a hand-RD walk):
//!
//!   chumsky parser over `&str`
//!       └─ each combinator returns `Vec<Green>` (an *intermediate* lossless
//!          tree of tokens + nodes, carrying byte ranges and interleaved
//!          trivia tokens)
//!   → a ~12-line post-pass feeds that intermediate into `GreenNodeBuilder`
//!     (rowan's builder is imperative/streaming; chumsky is bottom-up, so an
//!     intermediate is required — see SPIKE.md).
//!
//! No lowering happens. The CST is the deliverable.

use std::ops::Range;

use chumsky::prelude::*;
use rowan::{GreenNode, GreenNodeBuilder};

// ─────────────────────────────── SyntaxKind ───────────────────────────────

/// Token + node kinds for the slice. Screaming-case mirrors RW's `SyntaxKind`
/// (rust-analyzer convention). Discriminants are contiguous from 0 so
/// `from_raw` can `transmute` the `u16` rowan round-trips back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u16)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
enum SyntaxKind {
    // trivia (lexer)
    WHITESPACE,
    COMMENT,
    // leaf tokens
    IDENT,
    INT,
    FLOAT,
    STRING_FRAGMENT,
    DOLLAR_BRACE,
    R_BRACE,
    PIPE,
    COLON,
    CMP_OP,
    KEYWORD,
    ERROR,
    // interior nodes
    ROOT,
    QUERY,
    SOURCE,
    METRIC_ID,
    DATASET,
    METRIC_NAME,
    FILTER_RULE,
    FILTER_ATOM,
    VALUE_FILTER,
    EXPR,
    STRING,
    ERROR_NODE, // must remain the LAST variant (bounds check in `from_raw`)
}

impl From<SyntaxKind> for rowan::SyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        Self(kind as u16)
    }
}

impl SyntaxKind {
    fn from_raw(raw: rowan::SyntaxKind) -> Self {
        assert!(
            raw.0 <= SyntaxKind::ERROR_NODE as u16,
            "raw syntax kind out of range"
        );
        // SAFETY: `#[repr(u16)]` with contiguous discriminants `0..=ERROR_NODE`;
        // the bounds check guarantees `raw.0` names a real variant, and rowan
        // only ever hands back kinds we produced.
        unsafe { std::mem::transmute::<u16, SyntaxKind>(raw.0) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum MplLang {}

impl rowan::Language for MplLang {
    type Kind = SyntaxKind;

    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        SyntaxKind::from_raw(raw)
    }

    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

type SyntaxNode = rowan::SyntaxNode<MplLang>;
type SyntaxToken = rowan::SyntaxToken<MplLang>;

// ─────────────────────── intermediate green tree ───────────────────────────

/// The value every combinator produces: a flat sequence of tree elements. A
/// node parser wraps its children's concatenated `Vec<Green>` in a single
/// `Green::Node`; trivia rides along as `Green::Token` elements so nothing is
/// dropped.
#[derive(Debug, Clone)]
enum Green {
    Token(SyntaxKind, Range<usize>),
    Node(SyntaxKind, Vec<Green>),
}

/// Wrap children into one node element.
fn node(kind: SyntaxKind, children: Vec<Green>) -> Vec<Green> {
    vec![Green::Node(kind, children)]
}

// ───────────────────────────── chumsky parser ──────────────────────────────

/// String context (`C`) is `String` so `Rich::custom(span, msg)` can carry a
/// human-readable recovery message (the default `C = ()` only accepts `()`).
type E<'a> = extra::Err<Rich<'a, char, SimpleSpan, String>>;

fn span_range(span: SimpleSpan) -> Range<usize> {
    span.start()..span.end()
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_'
}

fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn ident_pat<'a>() -> impl Parser<'a, &'a str, (), E<'a>> + Clone {
    any()
        .filter(|c: &char| is_ident_start(*c))
        .then(any().filter(|c: &char| is_ident_continue(*c)).repeated())
        .ignored()
}

/// Zero-or-more whitespace / `// comment` runs, each retained as a real trivia
/// token. This is the single point where chumsky's natural "throw trivia away"
/// behaviour (`.padded()` / `.ignored()`, used by the production `src/slice.rs`)
/// is replaced with "keep trivia as tokens" — the prerequisite for losslessness.
fn trivia<'a>() -> impl Parser<'a, &'a str, Vec<Green>, E<'a>> + Clone {
    let ws = any()
        .filter(|c: &char| c.is_whitespace())
        .repeated()
        .at_least(1)
        .to_slice()
        .map_with(|_s: &str, e| Green::Token(SyntaxKind::WHITESPACE, span_range(e.span())));
    let comment = just("//")
        .then(none_of('\n').repeated())
        .to_slice()
        .map_with(|_s: &str, e| Green::Token(SyntaxKind::COMMENT, span_range(e.span())));
    choice((ws, comment)).repeated().collect::<Vec<_>>()
}

/// A leaf token with any leading trivia attached *before* it. The token's byte
/// range is `inner`'s match only (trivia keeps its own ranges), so relabeling
/// is just "pick the `SyntaxKind`" — chumsky's analogue of RW's `bump_as`.
fn leaf<'a, O: 'a>(
    inner: impl Parser<'a, &'a str, O, E<'a>> + Clone + 'a,
    kind: SyntaxKind,
) -> impl Parser<'a, &'a str, Vec<Green>, E<'a>> + Clone {
    trivia()
        .then(
            inner
                .to_slice()
                .map_with(move |_s: &str, e| Green::Token(kind, span_range(e.span()))),
        )
        .map(|(mut leading, tok)| {
            leading.push(tok);
            leading
        })
}

/// A specific keyword (`where`, `filter`). Reads a full identifier and checks
/// the text — so `whereabouts` is NOT mistaken for `where` (boundary handling
/// for free, via greedy `ident_pat` + `try_map`).
fn kw<'a>(
    word: &'static str,
    kind: SyntaxKind,
) -> impl Parser<'a, &'a str, Vec<Green>, E<'a>> + Clone {
    let token = ident_pat()
        .to_slice()
        .try_map(move |s: &str, span| {
            if s == word {
                Ok(())
            } else {
                Err(Rich::custom(span, format!("expected `{word}`")))
            }
        })
        .map_with(move |(), e| Green::Token(kind, span_range(e.span())));
    trivia().then(token).map(|(mut leading, tok)| {
        leading.push(tok);
        leading
    })
}

fn cst_parser<'a>() -> impl Parser<'a, &'a str, Vec<Green>, E<'a>> {
    // `value` is recursive so a `${ … }` interpolation can contain a *nested*
    // string (`"${ "y" }"`) — exactly the construct that broke `logos` in RW
    // and forced its hand-written `expand_string`/`string_end`/`find_interp_close`
    // byte scanners. Here it is one `recursive` combinator.
    let value = recursive::<_, Vec<Green>, E<'a>, _, _>(|value| {
        let str_char = choice((
            just('\\').then(any()).ignored(),          // escape (incl. `\$`)
            just('$').then(just('{').not()).ignored(), // `$` not opening an interp
            none_of("\"\\$").ignored(),                // any other literal char
        ));

        // Opening `"` + leading literal text → one STRING_FRAGMENT (merges the
        // opening quote, like RW's boundary fragments).
        let open_frag = just('"')
            .then(str_char.repeated())
            .to_slice()
            .map_with(|_s: &str, e| {
                Green::Token(SyntaxKind::STRING_FRAGMENT, span_range(e.span()))
            });

        // A non-empty run of literal text between interpolations.
        let text_run = str_char
            .repeated()
            .at_least(1)
            .to_slice()
            .map_with(|_s: &str, e| {
                vec![Green::Token(
                    SyntaxKind::STRING_FRAGMENT,
                    span_range(e.span()),
                )]
            });

        // `${ <expr> }` — the interior is a real EXPR subtree (an ident, or a
        // nested value), NOT an opaque blob. Both `}` and the interior are
        // `.or_not()` so an *unterminated* interpolation still yields a tree.
        let interp_inner = choice((leaf(ident_pat(), SyntaxKind::IDENT), value.clone()))
            .map(|v| vec![Green::Node(SyntaxKind::EXPR, v)]);
        let interp = just("${")
            .map_with(|_, e| vec![Green::Token(SyntaxKind::DOLLAR_BRACE, span_range(e.span()))])
            .then(interp_inner.or_not())
            .then(leaf(just('}'), SyntaxKind::R_BRACE).or_not())
            .map(|((mut out, expr), rbrace)| {
                if let Some(expr) = expr {
                    out.extend(expr);
                }
                if let Some(rbrace) = rbrace {
                    out.extend(rbrace);
                }
                out
            });

        let body = choice((interp, text_run))
            .repeated()
            .collect::<Vec<Vec<Green>>>();
        let close = just('"')
            .map_with(|_, e| Green::Token(SyntaxKind::STRING_FRAGMENT, span_range(e.span())))
            .or_not();

        let string = trivia().then(open_frag).then(body).then(close).validate(
            |(((leading, open), body), close), e, emitter| {
                let mut children = vec![open];
                for part in body {
                    children.extend(part);
                }
                match close {
                    Some(c) => children.push(c),
                    None => emitter.emit(Rich::custom(e.span(), "unterminated string".to_string())),
                }
                let mut out = leading;
                out.push(Green::Node(SyntaxKind::STRING, children));
                out
            },
        );

        let digits = any()
            .filter(|c: &char| c.is_ascii_digit())
            .repeated()
            .at_least(1);
        let float = leaf(
            digits.then(just('.')).then(digits).ignored(),
            SyntaxKind::FLOAT,
        );
        let int = leaf(digits.ignored(), SyntaxKind::INT);
        let number = choice((float, int));

        choice((number, string)).boxed()
    });

    let cmp_op = leaf(
        choice((
            just("=="),
            just("!="),
            just("<="),
            just(">="),
            just("<"),
            just(">"),
        ))
        .ignored(),
        SyntaxKind::CMP_OP,
    );

    let value_filter = cmp_op
        .then(value.clone().or_not())
        .validate(|(cmp, val), e, emitter| {
            let mut children = cmp;
            match val {
                Some(v) => children.push(Green::Node(SyntaxKind::EXPR, v)),
                None => emitter.emit(Rich::custom(e.span(), "expected a value".to_string())),
            }
            node(SyntaxKind::VALUE_FILTER, children)
        });

    let tag = leaf(ident_pat(), SyntaxKind::IDENT);
    let filter_atom = tag
        .or_not()
        .then(value_filter.or_not())
        .validate(|(tag, vf), e, emitter| {
            let mut children = Vec::new();
            match tag {
                Some(t) => children.extend(t),
                None => {
                    emitter.emit(Rich::custom(e.span(), "expected a tag name".to_string()));
                }
            }
            if let Some(vf) = vf {
                children.extend(vf);
            }
            node(SyntaxKind::FILTER_ATOM, children)
        });

    let where_kw = choice((
        kw("where", SyntaxKind::KEYWORD),
        kw("filter", SyntaxKind::KEYWORD),
    ));
    let pipe = leaf(just('|'), SyntaxKind::PIPE);
    let filter_rule = pipe.then(where_kw).then(filter_atom).map(|((p, w), a)| {
        let mut children = p;
        children.extend(w);
        children.extend(a);
        node(SyntaxKind::FILTER_RULE, children)
    });

    let colon = leaf(just(':'), SyntaxKind::COLON);
    let dataset = leaf(ident_pat(), SyntaxKind::IDENT).map(|v| node(SyntaxKind::DATASET, v));
    let metric_name =
        leaf(ident_pat(), SyntaxKind::IDENT).map(|v| node(SyntaxKind::METRIC_NAME, v));
    let metric_id = dataset
        .then(colon.or_not())
        .then(metric_name.or_not())
        .validate(|((d, c), m), e, emitter| {
            let mut children = d;
            match c {
                Some(c) => children.extend(c),
                None => emitter.emit(Rich::custom(e.span(), "expected `:`".to_string())),
            }
            match m {
                Some(m) => children.extend(m),
                None => {
                    emitter.emit(Rich::custom(e.span(), "expected a metric name".to_string()));
                }
            }
            node(SyntaxKind::METRIC_ID, children)
        });
    let source = metric_id.map(|v| node(SyntaxKind::SOURCE, v));

    let query = source
        .then(filter_rule.repeated().collect::<Vec<Vec<Green>>>())
        .map(|(s, rules)| {
            let mut children = s;
            for rule in rules {
                children.extend(rule);
            }
            node(SyntaxKind::QUERY, children)
        });

    // `file = trivia query? trivia leftover` — the trailing `any().repeated()`
    // mops up anything the structured grammar could not consume into a single
    // ERROR_NODE, which is what makes the whole parse TOTAL and lossless even on
    // garbage input.
    trivia()
        .then(query.or_not())
        .then(trivia())
        .then(any().repeated().to_slice().map_with(|s: &str, e| {
            if s.is_empty() {
                Vec::new()
            } else {
                vec![Green::Node(
                    SyntaxKind::ERROR_NODE,
                    vec![Green::Token(SyntaxKind::ERROR, span_range(e.span()))],
                )]
            }
        }))
        .validate(|(((leading, query), trailing), leftover), e, emitter| {
            let mut children = leading;
            if let Some(q) = query {
                children.extend(q);
            }
            children.extend(trailing);
            if !leftover.is_empty() {
                emitter.emit(Rich::custom(
                    e.span(),
                    "unexpected trailing input".to_string(),
                ));
            }
            children.extend(leftover);
            node(SyntaxKind::ROOT, children)
        })
}

// ──────────────────────── intermediate → rowan ─────────────────────────────

/// A parse diagnostic, mirroring RW's `SyntaxError`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SyntaxError {
    message: String,
    range: Range<usize>,
}

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

/// Feed the intermediate tree into rowan's streaming builder. This is the whole
/// "glue" cost of bridging chumsky → rowan: ~12 lines.
fn emit(builder: &mut GreenNodeBuilder<'static>, green: &Green, src: &str) {
    match green {
        Green::Token(kind, range) => builder.token((*kind).into(), &src[range.clone()]),
        Green::Node(kind, children) => {
            builder.start_node((*kind).into());
            for child in children {
                emit(builder, child, src);
            }
            builder.finish_node();
        }
    }
}

fn parse(src: &str) -> Parse {
    let (out, errs) = cst_parser().parse(src).into_output_errors();
    // The root parser yields exactly `[Green::Node(ROOT, …)]`; fall back to an
    // empty ROOT only if chumsky somehow produced no output (it never does here,
    // because the grammar is total).
    let root = out
        .and_then(|mut v| (!v.is_empty()).then(|| v.remove(0)))
        .unwrap_or_else(|| Green::Node(SyntaxKind::ROOT, Vec::new()));

    let mut builder = GreenNodeBuilder::new();
    emit(&mut builder, &root, src);
    let green = builder.finish();

    let errors = errs
        .into_iter()
        .map(|e| {
            let span = *e.span();
            SyntaxError {
                message: e.to_string(),
                range: span.start()..span.end(),
            }
        })
        .collect();

    Parse { green, errors }
}

// ──────────────────────────── test utilities ───────────────────────────────

/// Concatenate the text of every token in the tree (lossless reconstruction).
/// Independent of rowan's `SyntaxNode::text()` so the round-trip proof does not
/// lean on a single rowan convenience method.
fn concat_tokens(node: &SyntaxNode) -> String {
    node.descendants_with_tokens()
        .filter_map(|el| el.into_token())
        .map(|t| t.text().to_string())
        .collect()
}

fn find_node(root: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxNode> {
    root.descendants().find(|n| n.kind() == kind)
}

fn first_token(node: &SyntaxNode, kind: SyntaxKind) -> Option<SyntaxToken> {
    node.descendants_with_tokens()
        .filter_map(|el| el.into_token())
        .find(|t| t.kind() == kind)
}

/// Parse, assert byte-for-byte lossless round-trip two independent ways, return
/// the parse + root node for further structural assertions.
fn round_trip(src: &str) -> (Parse, SyntaxNode) {
    let parsed = parse(src);
    let root = parsed.syntax();
    // 1. rowan's own reconstruction.
    assert_eq!(
        root.text().to_string(),
        src,
        "rowan SyntaxNode::text() must reproduce the input"
    );
    // 2. manual token-text concatenation.
    assert_eq!(
        concat_tokens(&root),
        src,
        "manual token concatenation must reproduce the input"
    );
    assert_eq!(root.kind(), SyntaxKind::ROOT);
    (parsed, root)
}

// ─────────────────────────────── the tests ─────────────────────────────────

/// PROPERTY 1 — lossless round-trip including trivia (leading comment, interior
/// spaces, trailing space) on a fully-valid slice query, with the expected node
/// shape (QUERY → SOURCE → METRIC_ID, plus a FILTER_RULE).
#[test]
fn happy_path_is_lossless_and_structured() {
    let src = "// leading comment\nds:metric | where region == 42 ";
    let (parsed, root) = round_trip(src);

    assert!(
        parsed.errors().is_empty(),
        "valid input must not produce diagnostics, got: {:?}",
        parsed.errors()
    );

    // Structure is real, not a token soup.
    let query = find_node(&root, SyntaxKind::QUERY).expect("QUERY node");
    let source = find_node(&query, SyntaxKind::SOURCE).expect("SOURCE node");
    let metric_id = find_node(&source, SyntaxKind::METRIC_ID).expect("METRIC_ID node");
    assert_eq!(
        first_token(&metric_id, SyntaxKind::IDENT).map(|t| t.text().to_string()),
        Some("ds".to_string()),
        "DATASET ident"
    );
    let filter = find_node(&query, SyntaxKind::FILTER_RULE).expect("FILTER_RULE node");
    assert!(
        first_token(&filter, SyntaxKind::KEYWORD).is_some(),
        "`where` keyword"
    );
    assert!(
        first_token(&filter, SyntaxKind::CMP_OP).is_some(),
        "`==` cmp op"
    );

    // Trivia survives as real tokens.
    assert!(
        root.descendants_with_tokens()
            .filter_map(|el| el.into_token())
            .any(|t| t.kind() == SyntaxKind::COMMENT && t.text() == "// leading comment"),
        "the line comment must be a COMMENT token"
    );
}

/// PROPERTY 2 — the `${ ident }` interior is addressable as its own
/// nodes/tokens (EXPR → IDENT), NOT one opaque STRING blob; and the whole thing
/// round-trips, interpolation interior included.
#[test]
fn interpolation_interior_is_addressable() {
    let src = r#"ds:metric | where host == "a ${ b } c""#;
    let (parsed, root) = round_trip(src);
    assert!(
        parsed.errors().is_empty(),
        "valid interpolation must not error, got: {:?}",
        parsed.errors()
    );

    let string = find_node(&root, SyntaxKind::STRING).expect("STRING node");

    // It is a NODE with structure, not a single token.
    assert!(
        string.children().next().is_some() || string.children_with_tokens().count() > 1,
        "STRING must be a structured node, not an opaque blob"
    );

    // The interpolation delimiters are addressable tokens.
    assert!(
        first_token(&string, SyntaxKind::DOLLAR_BRACE).is_some(),
        "`${{` is its own token"
    );
    assert!(
        first_token(&string, SyntaxKind::R_BRACE).is_some(),
        "`}}` is its own token"
    );

    // The interior expression is a real EXPR subtree containing IDENT `b`.
    let expr = find_node(&string, SyntaxKind::EXPR).expect("EXPR node inside STRING");
    assert_eq!(
        first_token(&expr, SyntaxKind::IDENT).map(|t| t.text().to_string()),
        Some("b".to_string()),
        "interpolated ident `b` is addressable"
    );

    // And the literal text fragments carry the surrounding quotes / spaces.
    let fragments: Vec<String> = string
        .descendants_with_tokens()
        .filter_map(|el| el.into_token())
        .filter(|t| t.kind() == SyntaxKind::STRING_FRAGMENT)
        .map(|t| t.text().to_string())
        .collect();
    assert!(
        fragments.iter().any(|f| f.starts_with('"')),
        "a fragment carries the opening quote, got {fragments:?}"
    );
}

/// BONUS — a *nested* string inside an interpolation (`"${ "y" }"`). This is the
/// recursive construct that broke `logos` in RW; chumsky handles it with one
/// `recursive` combinator and it still round-trips.
#[test]
fn nested_string_in_interpolation_round_trips() {
    let src = r#"ds:metric | where host == "x ${ "y" } z""#;
    let (parsed, root) = round_trip(src);
    assert!(
        parsed.errors().is_empty(),
        "valid nested string must not error, got: {:?}",
        parsed.errors()
    );

    let outer = find_node(&root, SyntaxKind::STRING).expect("outer STRING");
    let expr = find_node(&outer, SyntaxKind::EXPR).expect("EXPR in interpolation");
    let inner = find_node(&expr, SyntaxKind::STRING).expect("nested STRING in EXPR");
    assert!(
        concat_tokens(&inner).contains('y'),
        "nested string keeps its content"
    );
}

/// PROPERTY 3a — error recovery on an INCOMPLETE input. `ds:metric | where `
/// (missing tag/value) must still produce a tree with error markers and round
/// trip losslessly, including the trailing space.
#[test]
fn recovery_incomplete_where_clause() {
    let src = "ds:metric | where ";
    let (parsed, root) = round_trip(src);

    assert!(
        !parsed.errors().is_empty(),
        "incomplete `where` must record a diagnostic"
    );
    // The FILTER_RULE is still in the tree (recovery, not abort).
    assert!(
        find_node(&root, SyntaxKind::FILTER_RULE).is_some(),
        "FILTER_RULE survives recovery"
    );
    // The source before it parsed cleanly.
    assert!(find_node(&root, SyntaxKind::METRIC_ID).is_some());
}

/// PROPERTY 3b — error recovery on an UNTERMINATED string with an unterminated
/// interpolation: `"a ${ b`. Must still produce a tree (with an
/// `unterminated string` marker), keep the interior `b` addressable, and round
/// trip losslessly.
#[test]
fn recovery_unterminated_string() {
    let src = r#"ds:metric | where host == "a ${ b"#;
    let (parsed, root) = round_trip(src);

    assert!(
        parsed
            .errors()
            .iter()
            .any(|e| e.message.contains("unterminated string")),
        "must flag the unterminated string, got: {:?}",
        parsed.errors()
    );

    // Even unterminated, the interior is still structured.
    let string = find_node(&root, SyntaxKind::STRING).expect("STRING node");
    assert!(first_token(&string, SyntaxKind::DOLLAR_BRACE).is_some());
    let expr = find_node(&string, SyntaxKind::EXPR).expect("EXPR inside unterminated string");
    assert_eq!(
        first_token(&expr, SyntaxKind::IDENT).map(|t| t.text().to_string()),
        Some("b".to_string()),
        "interior `b` stays addressable mid-edit"
    );
}

/// PROPERTY 3c — total parser on outright garbage trailing input still round
/// trips (everything lands in an ERROR_NODE).
#[test]
fn recovery_trailing_garbage() {
    let src = "ds:metric @@@ ??? \u{2603}";
    let (parsed, root) = round_trip(src);
    assert!(
        !parsed.errors().is_empty(),
        "garbage must record a diagnostic"
    );
    assert!(
        find_node(&root, SyntaxKind::ERROR_NODE).is_some(),
        "trailing garbage is captured in an ERROR_NODE"
    );
}

/// Pure-trivia inputs (whitespace + comment only) round-trip too.
#[test]
fn trivia_only_round_trips() {
    for src in ["", "   ", "// just a comment", "\n\t // c\n  "] {
        let parsed = parse(src);
        let root = parsed.syntax();
        assert_eq!(
            root.text().to_string(),
            src,
            "trivia-only round trip for {src:?}"
        );
    }
}
