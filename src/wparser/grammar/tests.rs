use std::collections::HashMap;

use super::parse_file;
use crate::{
    compile_winnow,
    query::{Aggregate, Cmp, Expr, Filter, FilterOrIfDef, Query, Time, TimeUnit},
    types::Parameterized,
};

fn simple(query: &str) -> Query {
    let (q, _warnings) = compile_winnow(query, HashMap::new()).expect("should compile");
    q
}

fn only_filter(query: &str) -> Filter {
    match simple(query) {
        Query::Simple { filters, .. } => {
            assert_eq!(filters.len(), 1, "expected exactly one filter");
            filters[0].filter().clone()
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

// ── source ──────────────────────────────────────────────────────

#[test]
fn parses_bare_source() {
    match simple("ds:metric") {
        Query::Simple { source, .. } => {
            assert_eq!(source.metric_id.dataset.to_string(), "ds");
            assert_eq!(source.metric_id.metric.to_string(), "metric");
            assert!(source.time.is_none());
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

#[test]
fn parses_relative_time_range() {
    match simple("ds:metric[1h..30m]") {
        Query::Simple { source, .. } => {
            let range = source.time.expect("time range");
            assert!(matches!(
                range.start,
                Time::Relative(ref t) if t.value == 1 && t.unit == TimeUnit::Hour
            ));
            assert!(matches!(
                range.end,
                Some(Time::Relative(ref t)) if t.value == 30 && t.unit == TimeUnit::Minute
            ));
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

#[test]
fn parses_open_ended_time_range() {
    match simple("ds:metric[5m..]") {
        Query::Simple { source, .. } => {
            let range = source.time.expect("time range");
            assert!(range.end.is_none());
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

#[test]
fn parses_as_rename_in_source() {
    match simple("ds:metric as renamed") {
        Query::Simple { aggregates, .. } => {
            assert!(matches!(aggregates.first(), Some(Aggregate::As(_))));
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

#[test]
fn parses_escaped_idents() {
    match simple("`weird-ds`:`weird metric`") {
        Query::Simple { source, .. } => {
            assert_eq!(source.metric_id.dataset.to_string(), "weird-ds");
            assert_eq!(source.metric_id.metric.to_string(), "weird metric");
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

// ── filters ─────────────────────────────────────────────────────

#[test]
fn parses_value_filter() {
    let filter = only_filter(r#"ds:metric | filter region == "us""#);
    assert!(matches!(
        filter,
        Filter::Cmp {
            rhs: Cmp::Eq(Expr::Const(_)),
            ..
        }
    ));
}

#[test]
fn parses_regex_filter() {
    let filter = only_filter("ds:metric | filter region == #/us-.*/");
    assert!(matches!(
        filter,
        Filter::Cmp {
            rhs: Cmp::RegEx(Parameterized::Concrete(_)),
            ..
        }
    ));
}

#[test]
fn parses_is_filter() {
    let filter = only_filter("ds:metric | filter latency is float");
    assert!(matches!(
        filter,
        Filter::Cmp {
            rhs: Cmp::Is(_),
            ..
        }
    ));
}

#[test]
fn parses_and_or_not_with_parens() {
    let filter = only_filter("ds:metric | filter (a == \"1\" or b == \"2\") and not c == \"3\"");
    let Filter::And(items) = filter else {
        panic!("expected And, got {filter:?}");
    };
    assert_eq!(items.len(), 2);
    assert!(matches!(items[0], Filter::Or(_)));
    assert!(matches!(items[1], Filter::Not(_)));
}

#[test]
fn where_is_an_alias_for_filter() {
    let filter = only_filter(r#"ds:metric | where region == "us""#);
    assert!(matches!(
        filter,
        Filter::Cmp {
            rhs: Cmp::Eq(_),
            ..
        }
    ));
}

// ── the `== $param` vs `== #/regex/` ambiguity ──────────────────

#[test]
fn eq_regex_param_is_rewritten_to_regex_cmp() {
    // `tag == $re` parses as `Cmp::Eq(Param)`, then the shared typecheck pass
    // rewrites it to `Cmp::RegEx` because `$re` is `Regex`-typed. This is the
    // exact ambiguity pest defers to a later pass.
    let filter = only_filter("param $re: Regex;\nds:metric | filter tag == $re");
    assert!(
        matches!(
            filter,
            Filter::Cmp {
                rhs: Cmp::RegEx(Parameterized::Param { .. }),
                ..
            }
        ),
        "got {filter:?}"
    );
}

#[test]
fn eq_string_param_stays_value_cmp() {
    let filter = only_filter("param $s: string;\nds:metric | filter tag == $s");
    assert!(
        matches!(
            filter,
            Filter::Cmp {
                rhs: Cmp::Eq(Expr::Param { .. }),
                ..
            }
        ),
        "got {filter:?}"
    );
}

// ── align ───────────────────────────────────────────────────────

#[test]
fn parses_align_to_using() {
    match simple("ds:metric | align to 1m using sum") {
        Query::Simple { aggregates, .. } => {
            assert!(matches!(aggregates.as_slice(), [Aggregate::Align(a)] if a.time.is_some()));
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

#[test]
fn align_without_to_has_no_time() {
    match simple("ds:metric | align using sum") {
        Query::Simple { aggregates, .. } => {
            assert!(matches!(aggregates.as_slice(), [Aggregate::Align(a)] if a.time.is_none()));
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

#[test]
fn align_over_is_unsupported() {
    // mirrors pest: the sliding-window form is rejected (NotImplemented).
    assert!(compile_winnow("ds:metric | align over 1m using sum", HashMap::new()).is_err());
}

#[test]
fn unknown_align_function_errors() {
    assert!(compile_winnow("ds:metric | align using nope_fn", HashMap::new()).is_err());
}

// ── params ──────────────────────────────────────────────────────

#[test]
fn parses_param_declarations_and_option() {
    let query = "param $env: string;\nparam $opt: Option<int>;\nds:metric";
    match simple(query) {
        Query::Simple { params, .. } => {
            assert_eq!(params.len(), 2);
            assert_eq!(params[0].name, "env");
            assert_eq!(params[1].name, "opt");
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

#[test]
fn duplicate_param_is_reported() {
    let out = parse_file("param $x: int;\nparam $x: int;\nds:metric", Vec::new());
    assert!(!out.errors.is_empty(), "duplicate param should be an error");
}

#[test]
fn legacy_duration_param_warns() {
    let out = parse_file("param $d: duration;\nds:metric", Vec::new());
    assert!(out.errors.is_empty());
    assert!(!out.warnings.is_empty(), "legacy `duration` should warn");
}

// ── error recovery (the headline diagnostics axis) ──────────────

#[test]
fn recovers_at_pipe_boundaries_collecting_multiple_errors() {
    // Two malformed clauses surrounded by valid structure. Recovery resyncs at
    // each `|`, so BOTH errors are reported and the trailing valid filter is
    // still parsed into the AST.
    let out = parse_file(
        r#"ds:metric | filter region == | align oops | filter env == "p""#,
        Vec::new(),
    );
    assert!(out.query.is_some(), "should still build a best-effort AST");
    assert_eq!(out.errors.len(), 2, "got: {:#?}", out.errors);

    let Some(Query::Simple { filters, .. }) = out.query else {
        panic!("expected simple query");
    };
    // Only the trailing well-formed `env == "p"` filter survives.
    assert_eq!(filters.len(), 1);
    assert!(matches!(
        filters[0],
        FilterOrIfDef::Filter(Filter::Cmp { ref field, .. }) if field == "env"
    ));
}

#[test]
fn incomplete_filter_rhs_records_error_without_panicking() {
    let out = parse_file("ds:metric | filter region == ", Vec::new());
    assert!(out.query.is_some());
    assert!(!out.errors.is_empty());
}

#[test]
fn error_span_points_into_the_source() {
    let out = parse_file("ds:metric | align using nope_fn", Vec::new());
    let err = out.errors.first().expect("an error");
    // The unsupported function span must land on `nope_fn`.
    let rendered = format!("{err:?}");
    assert!(rendered.contains("nope_fn"), "got: {rendered}");
}

// ── representative query structure ──────────────────────────────

/// Formerly compared the `winnow` AST against the `pest` AST; `pest` is gone,
/// so this now pins the parsed structure of a representative query (the
/// `Display` impl redacts string constants for PII safety, so it is not
/// round-trippable).
#[test]
fn representative_query_structure() {
    let query =
        r#"ds:metric[5m..] | filter region == "us" and tag != "bad" | align to 1m using sum"#;
    match simple(query) {
        Query::Simple {
            source,
            filters,
            aggregates,
            ..
        } => {
            assert_eq!(source.metric_id.metric.to_string(), "metric");
            assert!(source.time.is_some());
            assert!(matches!(
                filters.as_slice(),
                [FilterOrIfDef::Filter(Filter::And(_))]
            ));
            assert!(matches!(aggregates.as_slice(), [Aggregate::Align(a)] if a.time.is_some()));
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

// ── newly-ported constructs (compensating for the deleted pest tests) ──

#[test]
fn parses_timestamp_and_rfc3339_and_modifier_times() {
    for (q, has_end) in [
        ("ds:metric[1747077736092..]", false),
        ("ds:metric[2025-03-01T13:00:00Z..+1h]", true),
    ] {
        match simple(q) {
            Query::Simple { source, .. } => {
                let range = source.time.expect("time range");
                assert_eq!(range.end.is_some(), has_end, "for {q}");
            }
            Query::Compute { .. } => panic!("expected simple query"),
        }
    }
}

#[test]
fn parses_map_eval_and_map_fn_and_group_and_bucket() {
    assert!(matches!(
        only_aggregate("ds:metric | map * 100"),
        Aggregate::Map(_)
    ));
    assert!(matches!(
        only_aggregate("ds:metric | map filter::gt(1)"),
        Aggregate::Map(_)
    ));
    assert!(matches!(
        only_aggregate("ds:metric | group by a, b using sum"),
        Aggregate::GroupBy(_)
    ));
    assert!(matches!(
        only_aggregate("ds:metric | bucket by a to 5m using histogram(max)"),
        Aggregate::Bucket(_)
    ));
}

fn only_aggregate(query: &str) -> Aggregate {
    match simple(query) {
        Query::Simple { mut aggregates, .. } => {
            assert_eq!(aggregates.len(), 1, "expected one aggregate");
            aggregates.remove(0)
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

#[test]
fn join_and_replace_are_not_supported() {
    assert!(compile_winnow("ds:metric | join a from b:c by a", HashMap::new()).is_err());
    assert!(compile_winnow("ds:metric | replace a = b ~ #s/x/y/", HashMap::new()).is_err());
}

#[test]
fn parses_compute_query() {
    let q = simple("( a:m1 | group using sum, b:m2 | group using sum ) | compute out using /");
    assert!(matches!(q, Query::Compute { .. }), "got {q:?}");
}

#[test]
fn parses_ifdef_else_and_extend_and_directives() {
    let q = simple(concat!(
        "set strict;\n",
        "param $tag: Option<string>;\n",
        "ds:metric | ifdef($tag) { where t == $tag } else { where t == \"d\" } | extend k = true",
    ));
    match q {
        Query::Simple {
            filters,
            extends,
            directives,
            ..
        } => {
            assert!(matches!(filters.as_slice(), [FilterOrIfDef::Ifdef { .. }]));
            assert_eq!(extends.len(), 1);
            assert!(directives.contains_key("strict"));
        }
        Query::Compute { .. } => panic!("expected simple query"),
    }
}

#[test]
fn parses_string_interpolation_into_fragments() {
    let q = simple("param $h: string;\nds:metric | extend u = \"a${ $h }b\"");
    let Query::Simple { extends, .. } = q else {
        panic!("expected simple query");
    };
    assert!(
        matches!(&extends[0].value, Expr::String(parts) if parts.len() == 3),
        "got {:?}",
        extends[0].value
    );
}

#[test]
fn param_value_parses_each_declared_type() {
    use crate::query::{ParamDeclaration, ParamType, ParamValue, TerminalParamType};
    use miette::SourceSpan;

    let decl = |typ| ParamDeclaration {
        span: SourceSpan::from(0..0),
        name: "p".to_string(),
        typ,
    };
    let dur = decl(ParamType::Terminal(TerminalParamType::Duration));
    assert!(matches!(
        crate::wparser::parse_param_value(&dur, "42s"),
        Ok(ParamValue::Duration(_))
    ));
    assert!(crate::wparser::parse_param_value(&dur, "nope").is_err());
    let re = decl(ParamType::Terminal(TerminalParamType::Regex));
    assert!(matches!(
        crate::wparser::parse_param_value(&re, "#/[a-z]+/"),
        Ok(ParamValue::Regex(_))
    ));
}
