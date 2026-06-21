//! Parsing + lowering tests for the representative slice.

use super::{SyntaxKind, lower::lower, parse};
use crate::query::{Aggregate, Cmp, Expr, Filter, FilterOrIfDef, Query, Time, TimeUnit};

/// Render the tree as `KIND "text"` lines for snapshot-style assertions.
fn dump(input: &str) -> String {
    fn go(node: &super::SyntaxNode, depth: usize, out: &mut String) {
        use std::fmt::Write as _;
        for child in node.children_with_tokens() {
            match child {
                rowan::NodeOrToken::Node(n) => {
                    let _ = writeln!(out, "{}{:?}", "  ".repeat(depth), n.kind());
                    go(&n, depth + 1, out);
                }
                rowan::NodeOrToken::Token(t) => {
                    let _ = writeln!(out, "{}{:?} {:?}", "  ".repeat(depth), t.kind(), t.text());
                }
            }
        }
    }
    let mut out = String::new();
    go(&parse(input).syntax(), 0, &mut out);
    out
}

#[test]
fn lossless_roundtrip_preserves_every_byte() {
    // Comments and whitespace are real tokens, so the tree text equals input.
    let inputs = [
        "ds:cpu | filter region == \"eu\"",
        "// header\nds:cpu // trailing\n| align to 1m using avg\n",
        "param $dur: Duration;\nds:cpu | align over 5m using avg",
        "metric:cpu | filter region == ", // incomplete
    ];
    for input in inputs {
        let parsed = parse(input);
        assert_eq!(
            parsed.syntax().text(),
            input,
            "roundtrip failed for {input:?}"
        );
    }
}

#[test]
fn interpolated_string_roundtrips_losslessly() {
    // The lexer descends into `${ … }`, so the interior lives in the CST as
    // real tokens/subtrees. Re-emitting the tree must still equal the source
    // byte-for-byte — the prerequisite for a trivia-preserving formatter.
    let inputs = [
        r#"ds:cpu | where tag == "Hello ${ name }!""#,
        r#"ds:cpu | extend u = "a${ $h }b""#,
        r#"ds:cpu | extend u = "${ x }""#, // leading interpolation
        r#"ds:cpu | extend u = "x${ a }${ b }y""#, // adjacent (empty middle)
        r#"ds:cpu | where tag == "price \${ 5 }""#, // escaped: not interpolation
        r#"ds:cpu | extend u = "${ "nested ${ inner }" }""#, // nested interpolation
        r#"ds:cpu | extend u = """#,       // empty string
        "set title = \"a ${ x } b\";\nds:cpu", // interpolation in a directive
    ];
    for input in inputs {
        let parsed = parse(input);
        assert_eq!(
            parsed.syntax().text(),
            input,
            "interpolation roundtrip failed for {input:?}"
        );
    }
}

// Regression lock for the string-boundary bug, now FIXED by token-driven
// boundary detection (Option B). An escaped ident whose name contains `}` is
// valid MPL, but the old two-phase byte scanner (`string_end`/`find_interp_close`)
// only skipped `\` and `"` — it was blind to backtick idents, `#/regex/`
// literals and `//` comments, all of which can carry a `}` or `"`. So it
// stopped at the `}` inside the ident name and mis-detected the `${ … }`
// boundary (empty interpolation + an ERROR_NODE + 3 spurious errors). The new
// lexer lexes each `${ … }` interior with `logos` and counts brace *tokens*, so
// `` `a}b` `` is a single ESCAPED_IDENT and the `}` inside it is never a
// delimiter; the interior parses as one ESCAPED_IDENT with no errors.
#[test]
fn interpolation_with_braced_escaped_ident_parses_cleanly() {
    let input = r#"ds:cpu | where t == "x ${ `a}b` }""#;
    let parsed = parse(input);

    assert_eq!(parsed.syntax().text(), input, "must stay lossless");
    assert!(
        parsed.errors().is_empty(),
        "a valid escaped ident inside `${{ … }}` must not error; got {:?}",
        parsed.errors()
    );

    let escaped: Vec<String> = parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::ESCAPED_IDENT)
        .map(|t| t.text().to_string())
        .collect();
    assert_eq!(
        escaped,
        vec!["`a}b`".to_string()],
        "the `${{ … }}` interior must be the single escaped ident `a}}b`"
    );
}

// (a) The same class of bug, but the escaped ident carries a `"` instead of a
// `}`. The old byte scanner's `string_end`/`find_interp_close` toggled on every
// `"`, so the quote inside `` `a"b` `` was read as the string's closing quote
// and the boundary collapsed. Lexing the interior with `logos` makes `` `a"b` ``
// a single ESCAPED_IDENT, so the embedded `"` is part of that token, never a
// delimiter. The interior must be exactly that one escaped ident, no errors.
#[test]
fn interpolation_with_quoted_escaped_ident_parses_cleanly() {
    let input = r#"ds:cpu | where t == "x ${ `a"b` }""#;
    let parsed = parse(input);

    assert_eq!(parsed.syntax().text(), input, "must stay lossless");
    assert!(
        parsed.errors().is_empty(),
        "a valid escaped ident with a `\"` inside `${{ … }}` must not error; got {:?}",
        parsed.errors()
    );

    let escaped: Vec<String> = parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::ESCAPED_IDENT)
        .map(|t| t.text().to_string())
        .collect();
    assert_eq!(
        escaped,
        vec!["`a\"b`".to_string()],
        "the `${{ … }}` interior must be the single escaped ident `a\"b`"
    );
}

// (b) A multi-line interpolation whose interior has a `//` line comment
// containing a `}` before the *real* closing `}` on the next line. The old
// `find_interp_close` byte scanner had no notion of comments, so it would stop
// at the `}` inside the comment and mis-detect the boundary. Lexing the
// interior with `logos` makes the `// …}` a single COMMENT token, so its `}` is
// not a delimiter and the boundary is the real `}`. The interior may error on
// *semantics* (a bare `x` is not a complete expr value here), but the BOUNDARY
// must be right: the outer STRING must span to the final `"`, and the whole
// input must round-trip byte-for-byte.
#[test]
fn interpolation_with_commented_brace_finds_real_boundary() {
    let input = "ds:cpu | where t == \"x ${ x // note }\n} y\"";
    let parsed = parse(input);

    assert_eq!(parsed.syntax().text(), input, "must stay lossless");

    // The COMMENT token's `}` is folded into the comment, not a delimiter.
    let comment = parsed
        .syntax()
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .find(|t| t.kind() == SyntaxKind::COMMENT)
        .expect("the interior `//` comment is a single COMMENT token");
    assert_eq!(comment.text(), "// note }");

    // The outer STRING spans to the final `"` (the boundary is the real `}`,
    // not the one inside the comment): the node ends at the input's end.
    let string = parsed
        .syntax()
        .descendants()
        .find(|n| n.kind() == SyntaxKind::STRING)
        .expect("a STRING node");
    assert_eq!(
        usize::from(string.text_range().end()),
        input.len(),
        "the outer string must span to the final closing quote"
    );

    // The real `}` after the comment closed the interpolation, and the trailing
    // ` y\"` is a literal fragment — so there is exactly one DOLLAR_BRACE and one
    // R_BRACE, proving the boundary was detected token-by-token.
    let dollar_braces = string
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::DOLLAR_BRACE)
        .count();
    let r_braces = string
        .descendants_with_tokens()
        .filter_map(rowan::NodeOrToken::into_token)
        .filter(|t| t.kind() == SyntaxKind::R_BRACE)
        .count();
    assert_eq!((dollar_braces, r_braces), (1, 1));
}

#[test]
fn unterminated_interpolated_string_recovers_interior_structure() {
    // Mid-edit, no closing quote: the lexer must still descend into the string
    // so the CST carries the same interior shape it builds for a closed one —
    // `STRING_FRAGMENT` text, the `${` delimiter and the embedded expression as
    // a real `EXPR` subtree. This is what unblocks CST-driven highlighting and
    // completion classification on incomplete strings.
    let input = "ds:cpu | where x == \"a ${ b";
    let parsed = parse(input);

    // Lossless: every byte (incl. the dangling string) is still in the tree.
    assert_eq!(parsed.syntax().text(), input);

    let string = parsed
        .syntax()
        .descendants()
        .find(|n| n.kind() == SyntaxKind::STRING)
        .expect("an unterminated string still produces a STRING node");

    let kinds: Vec<SyntaxKind> = string
        .children_with_tokens()
        .filter_map(|e| match e {
            rowan::NodeOrToken::Token(t) if !t.kind().is_trivia() => Some(t.kind()),
            rowan::NodeOrToken::Node(n) => Some(n.kind()),
            rowan::NodeOrToken::Token(_) => None,
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            SyntaxKind::STRING_FRAGMENT, // "a
            SyntaxKind::DOLLAR_BRACE,    // ${
            SyntaxKind::EXPR,            // the embedded expression
        ],
    );

    // The embedded expression carries the half-typed identifier.
    let expr = string
        .children()
        .find(|n| n.kind() == SyntaxKind::EXPR)
        .expect("embedded EXPR");
    assert!(
        expr.descendants_with_tokens()
            .filter_map(rowan::NodeOrToken::into_token)
            .any(|t| t.kind() == SyntaxKind::IDENT && t.text() == "b"),
        "the interpolation expr keeps the in-progress `b` ident"
    );

    // Still flagged as unterminated, with the extent running to EOF (unchanged
    // from the opaque-ERROR-token behaviour it replaces).
    assert!(
        parsed.errors().iter().any(
            |e| e.message == "unterminated string" && usize::from(e.range.end()) == input.len()
        ),
        "unterminated string is still diagnosed over its full extent"
    );
}

#[test]
fn parses_full_slice_query() {
    let tree = dump("ds:cpu[5m..] | filter region == \"eu\" | align to 1m using avg");
    assert!(tree.contains("SOURCE"));
    assert!(tree.contains("TIME_RANGE"));
    assert!(tree.contains("FILTER_RULE"));
    assert!(tree.contains("ALIGN_RULE"));
    assert!(tree.contains("KEYWORD \"filter\""));
    assert!(tree.contains("KEYWORD \"align\""));
    assert!(tree.contains("CMP_OP \"==\""));
}

#[test]
fn lowers_simple_query() {
    let query = lower(&parse(
        "ds:cpu | filter region == \"eu\" | align to 1m using avg",
    ))
    .expect("should lower");
    let Query::Simple {
        source,
        filters,
        aggregates,
        ..
    } = query
    else {
        panic!("expected a simple query");
    };
    assert_eq!(source.metric_id.dataset.to_string(), "ds");
    assert_eq!(&*source.metric_id.metric, "cpu");
    assert_eq!(filters.len(), 1);
    assert!(matches!(
        &filters[0],
        FilterOrIfDef::Filter(Filter::Cmp { field, .. }) if field == "region"
    ));
    assert!(matches!(aggregates.as_slice(), [Aggregate::Align(_)]));
}

#[test]
fn lowers_time_range() {
    let query = lower(&parse("ds:cpu[1h..30m]")).expect("should lower");
    let range = query.time_range().expect("has time range");
    assert!(matches!(
        &range.start,
        Time::Relative(rt) if rt.value == 1 && rt.unit == TimeUnit::Hour
    ));
    assert!(matches!(
        range.end.as_ref(),
        Some(Time::Relative(rt)) if rt.value == 30 && rt.unit == TimeUnit::Minute
    ));
}

/// The `== #/regex/` vs `== $param` ambiguity that pest defers to a later
/// pass: the regex literal lexes distinctly, so the slice resolves it during
/// parsing — a literal regex becomes `Cmp::RegEx`, while a value/param keeps
/// `Cmp::Eq` (the existing typecheck pass promotes regex-typed params).
#[test]
fn regex_literal_vs_param_ambiguity() {
    let regex_q = lower(&parse("ds:cpu | filter region == #/eu-.*/")).expect("lower regex");
    let Query::Simple { filters, .. } = regex_q else {
        panic!("simple");
    };
    assert!(matches!(
        filters[0].filter(),
        Filter::Cmp {
            rhs: Cmp::RegEx(_),
            ..
        }
    ));

    let param_q =
        lower(&parse("param $re: Regex;\nds:cpu | filter region == $re")).expect("lower param");
    let Query::Simple { filters, .. } = param_q else {
        panic!("simple");
    };
    // Still `Eq(Param)` after parsing; `ParamTypecheckVisitor` promotes it.
    assert!(matches!(
        filters[0].filter(),
        Filter::Cmp {
            rhs: Cmp::Eq(Expr::Param { .. }),
            ..
        }
    ));
}

#[test]
fn is_filter_and_logic() {
    let query = lower(&parse(
        "ds:cpu | filter region == \"eu\" and not status is int",
    ))
    .expect("lower");
    let Query::Simple { filters, .. } = query else {
        panic!("simple");
    };
    match filters[0].filter() {
        Filter::And(parts) => {
            assert_eq!(parts.len(), 2);
            assert!(matches!(&parts[1], Filter::Not(_)));
        }
        other => panic!("expected And, got {other:?}"),
    }
}

// ── error recovery ───────────────────────────────────────────────

#[test]
fn recovers_from_incomplete_filter() {
    // No panic, the tree still holds every token, and an error is recorded.
    let parsed = parse("metric:cpu | filter region == ");
    assert_eq!(parsed.syntax().text(), "metric:cpu | filter region == ");
    assert!(
        !parsed.errors().is_empty(),
        "expected a recovery diagnostic"
    );
    // The recognised prefix is still structured.
    let tree = dump("metric:cpu | filter region == ");
    assert!(tree.contains("FILTER_RULE"));
    assert!(tree.contains("CMP_OP \"==\""));
}

#[test]
fn recovers_from_incomplete_align() {
    let parsed = parse("metric:cpu | align using ");
    assert_eq!(parsed.syntax().text(), "metric:cpu | align using ");
    assert!(!parsed.errors().is_empty());
}

#[test]
fn unknown_pipe_becomes_error_node() {
    // An unknown pipe keyword is kept as an ERROR_NODE, not dropped. (`map`
    // is now a fully-supported rule, so this uses a genuinely unknown one.)
    let parsed = parse("ds:cpu | frobnicate 5");
    assert_eq!(parsed.syntax().text(), "ds:cpu | frobnicate 5");
    let has_error_node = parsed
        .syntax()
        .descendants()
        .any(|n| n.kind() == SyntaxKind::ERROR_NODE);
    assert!(has_error_node);
}

#[test]
fn leading_garbage_does_not_panic() {
    for input in ["", "   ", "{{{}}}", "|||", "(", "ds:"] {
        let parsed = parse(input);
        assert_eq!(parsed.syntax().text(), input);
    }
}

// ── full-grammar lowering (replaces the deleted pest-internal tests) ──────

#[test]
fn lowers_timestamp_and_modifier_times() {
    // `[<unix>..+1h]`: an integer timestamp start and a `+`-modifier end.
    let query = lower(&parse("ds:cpu[1747077736092..+1h]")).expect("lower");
    let range = query.time_range().expect("time range");
    assert!(matches!(&range.start, Time::Timestamp(1_747_077_736_092)));
    assert!(matches!(range.end.as_ref(), Some(Time::Modifier(m)) if m == "+1h"));
}

#[test]
fn lowers_rfc3339_time() {
    let query = lower(&parse("ds:cpu[2025-03-01T13:00:00Z..]")).expect("lower");
    let range = query.time_range().expect("time range");
    assert!(matches!(&range.start, Time::RFC3339(_)));
}

#[test]
fn lowers_map_group_bucket() {
    let q = lower(&parse(
        "ds:cpu | map * 100 | group by a, b using sum | bucket by a to 5m using histogram(max)",
    ))
    .expect("lower");
    let Query::Simple { aggregates, .. } = q else {
        panic!("simple");
    };
    assert!(matches!(
        aggregates.as_slice(),
        [
            Aggregate::Map(_),
            Aggregate::GroupBy(_),
            Aggregate::Bucket(_)
        ]
    ));
}

#[test]
fn lowers_ifdef_sample_extend() {
    let q = lower(&parse(concat!(
        "param $t: Option<string>;\n",
        "ds:cpu | sample 0.5 | ifdef($t) { where tag == $t } | extend env = \"prod\"",
    )))
    .expect("lower");
    let Query::Simple {
        filters,
        extends,
        sample,
        ..
    } = q
    else {
        panic!("simple");
    };
    assert_eq!(sample, Some(0.5));
    assert!(matches!(filters.as_slice(), [FilterOrIfDef::Ifdef { .. }]));
    assert_eq!(extends.len(), 1);
}

#[test]
fn lowers_compute_query() {
    let q = lower(&parse("( ds:a , ds:b ) | compute total using + | map * 2")).expect("lower");
    let Query::Compute { aggregates, .. } = q else {
        panic!("compute");
    };
    assert!(matches!(aggregates.as_slice(), [Aggregate::Map(_)]));
}

#[test]
fn lowers_signed_inf_and_interpolation() {
    let q = lower(&parse(
        "param $h: string;\nds:cpu | where x == -inf | extend u = \"a${ $h }b\"",
    ))
    .expect("lower");
    let Query::Simple {
        filters, extends, ..
    } = q
    else {
        panic!("simple");
    };
    assert!(matches!(
        filters[0].filter(),
        Filter::Cmp {
            rhs: Cmp::Eq(Expr::Const(_)),
            ..
        }
    ));
    // The interpolated value keeps its fragments (text + param expr).
    assert!(matches!(&extends[0].value, Expr::String(_)));
}

#[test]
fn join_and_replace_lower_to_not_supported() {
    for q in [
        "ds:a | join b from ds:c by d",
        "ds:a | replace x = y ~ #s/a/b/",
    ] {
        assert!(lower(&parse(q)).is_err(), "{q} should be NotSupported");
    }
}

#[test]
fn escaped_param_ident_lowers() {
    // `$`backtick`` escaped param idents resolve like plain ones.
    let q = lower(&parse(
        "param $`weird name`: string;\nds:cpu | where tag == $`weird name`",
    ))
    .expect("lower");
    let Query::Simple { filters, .. } = q else {
        panic!("simple");
    };
    assert!(matches!(
        filters[0].filter(),
        Filter::Cmp {
            rhs: Cmp::Eq(Expr::Param { .. }),
            ..
        }
    ));
}
