//! Tests for the chumsky slice parser and highlighter.
//!
//! Coverage strategy:
//! * Equivalence — for valid slice queries the chumsky AST must be byte-for-byte
//!   identical (via serde) to what the existing `pest` pipeline produces.
//! * Recovery — incomplete / malformed input must still yield a best-effort AST
//!   plus errors (no panic, no total failure).
//! * Highlighting — must return tokens even on incomplete input.

use std::collections::HashMap;

use super::{Highlight, HlKind, highlight, parse};
use crate::{
    compile,
    query::{Cmp, Expr, Filter, FilterOrIfDef, Query, Time},
    types::Parameterized,
};

fn json(q: &Query) -> String {
    serde_json::to_string_pretty(q).expect("serialize")
}

/// Asserts the chumsky AST equals the pest+typecheck AST for a valid query.
fn assert_matches_pest(query: &str) {
    let slice = parse(query);
    assert!(slice.errors.is_empty(), "slice errors: {:?}", slice.errors);
    let slice_q = slice.query.expect("slice should produce a query");
    let (pest_q, _w) = compile(query, HashMap::new()).expect("pest should compile");
    assert_eq!(json(&slice_q), json(&pest_q), "AST mismatch for `{query}`");
}

fn first_filter(q: &Query) -> &Filter {
    match q {
        Query::Simple { filters, .. } => filters
            .first()
            .map(FilterOrIfDef::filter)
            .expect("a filter"),
        Query::Compute { .. } => panic!("compute not in slice"),
    }
}

// ───────────────────────── equivalence with pest ─────────────────────────

#[test]
fn equiv_source_only() {
    assert_matches_pest("ds:metric");
}

#[test]
fn equiv_escaped_idents() {
    assert_matches_pest("`my ds`:`my metric`");
}

#[test]
fn equiv_filter_string() {
    assert_matches_pest(r#"ds:metric | filter region == "us-east""#);
}

#[test]
fn equiv_filter_where_synonym() {
    assert_matches_pest(r#"ds:metric | where region != "x""#);
}

#[test]
fn equiv_filter_bool_int() {
    assert_matches_pest("ds:metric | filter ok == true");
    assert_matches_pest("ds:metric | filter n >= 5");
}

#[test]
fn equiv_filter_and_or_not_parens() {
    assert_matches_pest(r#"ds:metric | filter (a == "1" or b == "2") and not c == "3""#);
}

#[test]
fn equiv_filter_is() {
    assert_matches_pest("ds:metric | filter tag is string");
}

#[test]
fn equiv_filter_regex_literal() {
    assert_matches_pest("ds:metric | filter path == #/^abc$/");
}

#[test]
fn equiv_time_range_open() {
    assert_matches_pest("ds:metric[5m..]");
}

#[test]
fn equiv_time_range_closed() {
    assert_matches_pest("ds:metric[1h..30m]");
}

#[test]
fn equiv_as() {
    assert_matches_pest("ds:metric as renamed");
}

#[test]
fn equiv_align() {
    assert_matches_pest("ds:metric | align using avg");
    assert_matches_pest("ds:metric | align to 1m using avg");
}

#[test]
fn equiv_directive() {
    assert_matches_pest("set foo = 1;\nds:metric");
}

#[test]
fn equiv_comments_are_trivia() {
    assert_matches_pest("// a header comment\nds:metric | filter a == \"1\" // trailing");
}

// ───────────────────────── param resolution ─────────────────────────

#[test]
fn equiv_param_dataset() {
    assert_matches_pest("param $ds: Dataset;\n$ds:metric");
}

#[test]
fn equiv_param_duration_in_align() {
    assert_matches_pest("param $w: Duration;\nds:metric | align to $w using avg");
}

#[test]
fn undefined_param_is_reported_but_recovers() {
    let p = parse("ds:metric | filter region == $nope");
    assert!(p.query.is_some(), "should still build a query");
    assert!(
        p.errors.iter().any(|e| format!("{e:?}").contains("nope")),
        "should report the undefined param: {:?}",
        p.errors
    );
}

// ───────────── the `== #/regex/` vs `== $param` ambiguity ─────────────

#[test]
fn regex_literal_parses_as_regex_cmp() {
    let q = parse("ds:metric | filter path == #/abc/")
        .query
        .expect("query");
    match first_filter(&q) {
        Filter::Cmp {
            rhs: Cmp::RegEx(Parameterized::Concrete(_)),
            ..
        } => {}
        other => panic!("expected concrete RegEx cmp, got {other:?}"),
    }
}

#[test]
fn regex_param_defers_to_typecheck_like_pest() {
    // `== $re` cannot be distinguished from a value comparison at parse time
    // (the param's *type* is unknown to the grammar), so — exactly like pest —
    // chumsky produces `Cmp::Eq(Expr::Param)` and leaves the rewrite-to-RegEx
    // to the existing typecheck pass.
    let q = parse("param $re: Regex;\nds:metric | filter path == $re")
        .query
        .expect("query");
    match first_filter(&q) {
        Filter::Cmp {
            rhs: Cmp::Eq(Expr::Param { param, .. }),
            ..
        } => assert_eq!(param.name, "re"),
        other => panic!("expected deferred Eq(Param), got {other:?}"),
    }
    // And the full pest pipeline rewrites it to RegEx — proving our pre-typecheck
    // shape matches pest's pre-typecheck shape (same deferral contract).
    let (pest_q, _) = compile(
        "param $re: Regex;\nds:metric | filter path == $re",
        HashMap::new(),
    )
    .expect("compile");
    match first_filter(&pest_q) {
        Filter::Cmp {
            rhs: Cmp::RegEx(Parameterized::Param { .. }),
            ..
        } => {}
        other => panic!("pest should rewrite to RegEx(Param), got {other:?}"),
    }
}

// ───────────────────────── error recovery ─────────────────────────

#[test]
fn recovers_from_incomplete_filter_rhs() {
    // The headline incomplete-input case from the task.
    let p = parse("metric:cpu | filter region == ");
    assert!(p.query.is_some(), "source should still parse");
    assert!(!p.errors.is_empty(), "should report the missing rhs");
}

#[test]
fn recovers_from_incomplete_align() {
    let p = parse("metric:cpu | align using ");
    assert!(p.query.is_some());
    assert!(!p.errors.is_empty());
}

#[test]
fn collects_multiple_errors() {
    // Two bad clauses → both reported (multi-error), not just the first.
    let p = parse("ds:metric | filter a == | align using nope_fn");
    assert!(
        p.errors.len() >= 2,
        "expected multiple errors, got {:?}",
        p.errors
    );
}

#[test]
fn time_range_start_is_relative() {
    let q = parse("ds:metric[5m..]").query.expect("query");
    match &q {
        Query::Simple { source, .. } => {
            let tr = source.time.as_ref().expect("time range");
            assert!(matches!(tr.start, Time::Relative(_)));
            assert!(tr.end.is_none());
        }
        Query::Compute { .. } => unreachable!(),
    }
}

// ───────────────────────── highlighting ─────────────────────────

fn text<'a>(src: &'a str, t: &Highlight) -> &'a str {
    &src[t.from..t.to]
}

#[test]
fn highlight_full_filter_query() {
    let src = r#"ds:metric | filter region == "x""#;
    let toks = highlight(src);
    let kinds: Vec<_> = toks.iter().map(|t| (text(src, t), t.kind)).collect();
    assert!(kinds.contains(&("ds", HlKind::Variable)));
    assert!(kinds.contains(&("|", HlKind::Punctuation)));
    assert!(kinds.contains(&("filter", HlKind::Keyword)));
    assert!(kinds.contains(&("region", HlKind::Variable)));
    assert!(kinds.contains(&("==", HlKind::Operator)));
    assert!(kinds.contains(&(r#""x""#, HlKind::String)));
}

#[test]
fn highlight_survives_incomplete_input() {
    // No panic, tokens still returned — this is the core editor proof.
    for src in [
        "metric:cpu | filter region == ",
        "metric:cpu | align using ",
        r#"ds:metric | filter x == "unterminated"#,
        "param $d: Dur",
    ] {
        let toks = highlight(src);
        assert!(!toks.is_empty(), "no tokens for `{src}`");
    }
    // keyword still highlighted mid-edit
    let src = "metric:cpu | filter region == ";
    let toks = highlight(src);
    assert!(
        toks.iter()
            .any(|t| t.kind == HlKind::Keyword && text(src, t) == "filter")
    );
}

#[test]
fn highlight_comment_is_trivia_token() {
    let src = "// hello\nds:metric";
    let toks = highlight(src);
    assert!(
        toks.iter()
            .any(|t| t.kind == HlKind::Comment && text(src, t) == "// hello")
    );
}

#[test]
fn highlight_param_type_and_regex() {
    let src = "param $f: Option<string>;\nds:metric | filter p == #/x/";
    let toks = highlight(src);
    let kinds: Vec<_> = toks.iter().map(|t| (text(src, t), t.kind)).collect();
    assert!(kinds.contains(&("param", HlKind::Keyword)));
    assert!(kinds.contains(&("$f", HlKind::Variable)));
    assert!(kinds.contains(&("Option", HlKind::Type)));
    assert!(kinds.contains(&("string", HlKind::Type)));
    assert!(kinds.contains(&("#/x/", HlKind::Regexp)));
}

/// The highlighter descends into `${ … }` so an interpolated string is no
/// longer one opaque `String` token: text fragments (quotes merged in) stay
/// `String`, the embedded param is classified on its own. This is the 3-token
/// model — String / Variable / String — mirroring the parser's
/// `string_expr` text/interp split, but for highlighting only (CH has no
/// lossless CST, so this is not a formatter feature).
#[test]
fn highlight_string_interpolation_sub_tokens() {
    let src = r#"ds:metric | where h == "host ${ $h } end""#;
    let kinds: Vec<_> = highlight(src)
        .iter()
        .map(|t| (text(src, t), t.kind))
        .collect();
    assert!(kinds.contains(&(r#""host "#, HlKind::String)));
    assert!(kinds.contains(&("$h", HlKind::Variable)));
    assert!(kinds.contains(&(r#" end""#, HlKind::String)));
    // The full literal is NOT a single String token anymore.
    assert!(!kinds.contains(&(r#""host ${ $h } end""#, HlKind::String)));
}

/// A number embedded in `${ … }` highlights as `Number`, and a nested string
/// re-enters the same token set (the recursion the parser's `string_expr` also
/// relies on), proving there is no second highlight grammar.
#[test]
fn highlight_string_interpolation_number_and_nested() {
    let num = highlight(r#""n ${ 42 } m""#);
    let nk: Vec<_> = num
        .iter()
        .map(|t| (text(r#""n ${ 42 } m""#, t), t.kind))
        .collect();
    assert!(nk.contains(&("42", HlKind::Number)));

    let src = r#""a ${ "b ${ $c } d" } e""#;
    let kinds: Vec<_> = highlight(src)
        .iter()
        .map(|t| (text(src, t), t.kind))
        .collect();
    assert!(kinds.contains(&("$c", HlKind::Variable)));
    assert!(kinds.contains(&(r#""b "#, HlKind::String)));
}
