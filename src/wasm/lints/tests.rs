use super::detect_hints;
use crate::wasm::diagnostics::Severity;

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

// ── lowercase duration hint ──────────────────────────────────────

#[test]
fn lowercase_duration_warns() {
    let query = "param $t: duration;\nds:metric | align to $t over 5m using sum";
    let items = detect_hints(query);
    let duration_hints: Vec<_> = items
        .iter()
        .filter(|i| i.message.contains("Duration"))
        .collect();
    assert_eq!(duration_hints.len(), 1);
    assert!(matches!(duration_hints[0].severity, Severity::Warning));
    assert_eq!(duration_hints[0].actions.len(), 1);
    assert_eq!(duration_hints[0].actions[0].insert, "Duration");
    assert_eq!(
        &query[duration_hints[0].span.from..duration_hints[0].span.to],
        "duration"
    );
}

#[test]
fn uppercase_duration_no_hint() {
    let query = "param $t: Duration;\nds:metric | align to $t over 5m using sum";
    let items = detect_hints(query);
    assert!(
        !items.iter().any(|i| i.message.contains("Duration")),
        "uppercase Duration should not produce a lint"
    );
}

#[test]
fn lowercase_duration_fix_span_correct() {
    // Verify the action span covers exactly the `duration` token.
    let query = "param $t: duration;\nds:metric | align to $t over 5m using sum";
    let items = detect_hints(query);
    let hint = items
        .iter()
        .find(|i| i.message.contains("Duration"))
        .expect("expected a Duration hint");
    let action = &hint.actions[0];
    // The action span should replace the exact token.
    assert_eq!(&query[action.span.from..action.span.to], "duration");
    assert_eq!(action.insert, "Duration");
}

#[test]
fn multiple_lowercase_duration_params() {
    let query =
        "param $t: duration;\nparam $u: duration;\n$dataset:metric | align to $t over $u using sum";
    let items = detect_hints(query);
    let duration_hints: Vec<_> = items
        .iter()
        .filter(|i| i.message.contains("Duration"))
        .collect();
    assert_eq!(duration_hints.len(), 2, "should warn for each occurrence");
}

// ── dataset given, no metric ─────────────────────────────────────

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
    // `filter` keyword would normally produce a hint, but parse fails
    // so no hints are emitted
    assert!(detect_hints("ds: | filter tag == \"x\"").is_empty());
}

#[test]
fn no_hints_dataset_no_colon_with_filter() {
    assert!(detect_hints("ds | filter tag == \"x\"").is_empty());
}

#[test]
fn no_hints_backtick_dataset_no_metric_with_where() {
    assert!(detect_hints("`my-dataset`: | where tag == \"x\"").is_empty());
}
