use super::detect_hints;
use crate::diagnostics::Severity;

// ── filter keyword hint ─────────────────────────────────────────

#[test]
fn filter_hint() {
    let query = "ds:metric | filter tag == \"x\"";
    let items = detect_hints(query);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].severity, Severity::Hint));
    assert_eq!(items[0].actions.len(), 1);
    assert_eq!(items[0].actions[0].insert, "where");
    assert_eq!(&query[items[0].span.from..items[0].span.to], "filter");
}

#[test]
fn where_no_hint() {
    let query = "ds:metric | where tag == \"x\"";
    let items = detect_hints(query);
    assert!(items.is_empty(), "should not suggest for `where`");
}

#[test]
fn filter_hint_multiple() {
    let query = "ds:metric | filter a == \"1\" | filter b == \"2\"";
    let items = detect_hints(query);
    assert_eq!(items.len(), 2, "should detect both `filter` keywords");
}

#[test]
fn filter_hint_mixed_where_and_filter() {
    let query = "ds:metric | where a == \"1\" | filter b == \"2\"";
    let items = detect_hints(query);
    assert_eq!(items.len(), 1, "should only flag the `filter` keyword");
    assert_eq!(&query[items[0].span.from..items[0].span.to], "filter");
}

/// `filter` inside an `ifdef` body is still the deprecated alias and must
/// produce the same hint as anywhere else. Locks in the canonical-form
/// invariant: the ifdef body always reads as `where`, never `filter`.
#[test]
fn filter_hint_inside_ifdef_body() {
    let query = "param $f: Option<string>;\nds:metric | ifdef($f) { filter tag == $f }";
    let items = detect_hints(query);
    let filter_hints: Vec<_> = items
        .iter()
        .filter(|i| i.message.contains("filter"))
        .collect();
    assert_eq!(filter_hints.len(), 1, "filter inside ifdef should hint");
    assert_eq!(filter_hints[0].actions[0].insert, "where");
    assert_eq!(
        &query[filter_hints[0].span.from..filter_hints[0].span.to],
        "filter"
    );
}

// ── unnecessary escape lint ──────────────────────────────────────

#[test]
fn unnecessary_escape_plain_tag() {
    let query = "ds:metric | filter `tag` == \"x\"";
    let items = detect_hints(query);
    let escape_hints: Vec<_> = items
        .iter()
        .filter(|i| i.message.contains("backtick"))
        .collect();
    assert_eq!(escape_hints.len(), 1);
    assert!(matches!(escape_hints[0].severity, Severity::Hint));
    assert_eq!(escape_hints[0].actions.len(), 1);
    assert_eq!(escape_hints[0].actions[0].insert, "tag");
    assert_eq!(
        &query[escape_hints[0].span.from..escape_hints[0].span.to],
        "`tag`"
    );
}

#[test]
fn no_unnecessary_escape_for_hyphenated() {
    let query = "ds:metric | filter `my-tag` == \"x\"";
    let items = detect_hints(query);
    assert!(
        !items.iter().any(|i| i.message.contains("backtick")),
        "hyphenated ident needs escaping"
    );
}

#[test]
fn no_unnecessary_escape_for_leading_digit() {
    let query = "ds:metric | filter `0abc` == \"x\"";
    let items = detect_hints(query);
    assert!(
        !items.iter().any(|i| i.message.contains("backtick")),
        "leading-digit ident needs escaping"
    );
}

#[test]
fn no_unnecessary_escape_for_dotted() {
    let query = "ds:metric | filter `my_tag.name` == \"x\"";
    let items = detect_hints(query);
    assert!(
        !items.iter().any(|i| i.message.contains("backtick")),
        "dotted ident needs escaping"
    );
}

#[test]
fn unnecessary_escape_multiple() {
    let query = "ds:metric | filter `a` == \"1\" and `b` == \"2\"";
    let items = detect_hints(query);
    let escape_hints: Vec<_> = items
        .iter()
        .filter(|i| i.message.contains("backtick"))
        .collect();
    assert_eq!(escape_hints.len(), 2, "should flag both escaped idents");
}

// Note: lowercase `duration` warnings are emitted by the parser itself and
// covered by the wasm/diagnostics tests, not the post-parse lint pass.

// ── broken source ────────────────────────────────────────────────
//
// A lint fires on a word the parser recognised as a rule keyword. With the
// source broken it never gets far enough to build a `KEYWORD` node, so these
// stay silent.

#[test]
fn no_hints_dataset_colon_no_metric() {
    assert!(detect_hints("ds:").is_empty());
}

#[test]
fn no_hints_dataset_no_colon() {
    assert!(detect_hints("ds").is_empty());
}

#[test]
fn no_hints_dataset_no_metric_with_filter() {
    // Recovery consumes the `|` while reporting the missing metric, so what
    // follows is never read as a rule and `filter` is left as a bare word.
    assert!(detect_hints("ds: | filter tag == \"x\"").is_empty());
}

#[test]
fn no_hints_dataset_no_colon_with_filter() {
    // Here the `:` is what is missing, so `filter` is taken as the metric name.
    assert!(detect_hints("ds | filter tag == \"x\"").is_empty());
}

#[test]
fn no_hints_backtick_dataset_no_metric_with_where() {
    assert!(detect_hints("`my-dataset`: | where tag == \"x\"").is_empty());
}

/// The source is well formed and the `filter` is unambiguous, so the hint fires
/// without waiting for the user to finish the comparison.
#[test]
fn hint_on_a_query_that_does_not_yet_parse() {
    let query = "ds:metric | filter tag == ";
    let items = detect_hints(query);
    assert_eq!(items.len(), 1, "incomplete comparison should still hint");
    assert_eq!(items[0].actions[0].insert, "where");
    assert_eq!(&query[items[0].span.from..items[0].span.to], "filter");
}

/// A tag named after a rule keyword is not the keyword; the `KEYWORD` node is
/// what separates them.
#[test]
fn tag_named_filter_is_not_hinted() {
    assert!(
        detect_hints("ds:metric | where filter == \"x\"").is_empty(),
        "`filter` used as a tag name is not the deprecated keyword"
    );
}
