use regex::Regex;
use test_case::test_case;

use crate::stdlib::STDLIB;

use super::{
    CompletionResult, ParamType, QueryContext, compute_completions, extract_declared_params,
    extract_partial_word, is_char_escaped, locate_query_context,
};

fn tag_info(r: &CompletionResult) -> Option<(&str, &str)> {
    match r {
        CompletionResult::Tag {
            dataset, metric, ..
        } => Some((dataset.as_str(), metric.as_str())),
        _ => None,
    }
}

fn source_dataset(r: &CompletionResult) -> Option<&str> {
    match r {
        CompletionResult::Metric { dataset, .. } => Some(dataset.as_str()),
        _ => None,
    }
}

// ── locate_query_context ──────────────────────────────────────────

#[test]
fn context_simple_query() {
    let text = "ds:metric | filter ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, text),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_simple_query_with_directive() {
    let text = "set foo = bar; ds:metric | filter ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, text),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_compute_first_subquery() {
    let text = "( ds:metric | filter ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, " ds:metric | filter "),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_compute_second_subquery() {
    let text = "( ds1:m1 , ds2:m2 | filter ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, " ds2:m2 | filter "),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_compute_outer_first_pipe() {
    let text = "( ds1:m1 , ds2:m2 ) | ";
    match locate_query_context(text) {
        QueryContext::ComputeRulePipe(s) => assert_eq!(s, " | "),
        _ => panic!("expected ComputeRulePipe"),
    }
}

#[test]
fn context_compute_outer_tail_pipe() {
    let text = "( ds1:m1 , ds2:m2 ) | compute result using / | ";
    match locate_query_context(text) {
        QueryContext::ComputeTailPipe(s) => {
            assert_eq!(s, " | compute result using / | ");
        }
        _ => panic!("expected ComputeTailPipe"),
    }
}

#[test]
fn context_nested_compute_inner_subquery() {
    let text = "( ( a:b , c:d | filter ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, " c:d | filter "),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_nested_compute_outer_subquery() {
    let text = "( ( a:b , c:d ) | compute x using / , e:f | filter ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, " e:f | filter "),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_braces_inside_string_ignored() {
    let text = "ds:metric | filter tag == \"(,)\" | ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, text),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_braces_inside_backtick_ignored() {
    let text = "ds:metric | filter `tag(,)` == \"val\" | ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, text),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_braces_inside_regex_ignored() {
    let text = "ds:metric | filter tag == #/(,)/ | ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, text),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_pipe_inside_regex_ignored() {
    let text = "ds:metric | filter tag == #/a|b/ | ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, text),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_braces_inside_regex_replace_ignored() {
    let text = "ds:metric | replace tag ~ #s/(/[/ | ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, text),
        _ => panic!("expected Subquery"),
    }
}

// ── comment preceding compute paren ──────────────────────────────

#[test]
fn context_comment_before_compute_paren() {
    let text = "// comment\n( ds:metric | filter ";
    match locate_query_context(text) {
        QueryContext::Subquery(s) => assert_eq!(s, " ds:metric | filter "),
        _ => panic!("expected Subquery"),
    }
}

#[test]
fn context_directive_comment_before_compute_paren() {
    let text = "set foo = bar; // note\n( ds1:m1 , ds2:m2 ) | ";
    match locate_query_context(text) {
        QueryContext::ComputeRulePipe(s) => assert_eq!(s, " | "),
        _ => panic!("expected ComputeRulePipe"),
    }
}

// ── extract_partial_word (backtick support) ─────────────────────

#[test]
fn partial_word_plain_ident() {
    let text = "ds:metric | filter tag";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "tag");
    assert_eq!(start, text.len() - 3);
}

#[test]
fn partial_word_backtick_dataset_with_plain_metric() {
    let text = "`my-dataset`:metric";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "`my-dataset`:metric");
    assert_eq!(start, 0);
}

#[test]
fn partial_word_backtick_dataset_colon_only() {
    let text = "`my-dataset`:";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "`my-dataset`:");
    assert_eq!(start, 0);
}

#[test]
fn partial_word_both_backticked() {
    let text = "`my-dataset`:`my-metric`";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "`my-dataset`:`my-metric`");
    assert_eq!(start, 0);
}

#[test]
fn partial_word_backtick_after_space() {
    let text = "set foo = bar; `my-dataset`:met";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "`my-dataset`:met");
    assert_eq!(start, text.len() - "`my-dataset`:met".len());
}

#[test]
fn partial_word_backtick_with_escaped_backtick() {
    // Dataset name contains an escaped backtick: `my\`ds`
    let text = r"`my\`ds`:metric";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, r"`my\`ds`:metric");
    assert_eq!(start, 0);
}

#[test]
fn partial_word_empty_input() {
    let (start, partial) = extract_partial_word("", 0);
    assert_eq!(partial, "");
    assert_eq!(start, 0);
}

#[test]
fn partial_word_cursor_at_zero() {
    let (start, partial) = extract_partial_word("ds:metric", 0);
    assert_eq!(partial, "");
    assert_eq!(start, 0);
}

#[test]
fn partial_word_cursor_mid_word() {
    let text = "ds:metric";
    let (start, partial) = extract_partial_word(text, 5); // cursor after "ds:me"
    assert_eq!(partial, "ds:me");
    assert_eq!(start, 0);
}

#[test]
fn partial_word_single_backtick() {
    let text = "`";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "`");
    assert_eq!(start, 0);
}

#[test]
fn partial_word_single_backtick_after_space() {
    let text = "set x; `";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "`");
    assert_eq!(start, text.len() - 1);
}

#[test]
fn partial_word_closed_backtick_no_colon() {
    // Just a backtick-escaped dataset name, no colon yet
    let text = "set x; `my-dataset`";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "`my-dataset`");
    assert_eq!(start, text.len() - "`my-dataset`".len());
}

#[test]
fn partial_word_backtick_tag_in_filter() {
    let text = "ds:metric | filter `my-tag`";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "`my-tag`");
    assert_eq!(start, text.len() - "`my-tag`".len());
}

#[test]
fn partial_word_double_escaped_backslash_before_backtick() {
    // `name\\` — the \\\\ is an escaped backslash, so the closing backtick
    // is NOT escaped and should be recognised as a real closing backtick.
    let text = r"`name\\`";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, r"`name\\`");
    assert_eq!(start, 0);
}

// ── is_char_escaped ──────────────────────────────────────────────

#[test]
fn is_char_escaped_no_backslash() {
    assert!(!is_char_escaped(b"abc", 2)); // 'c' not escaped
}

#[test]
fn is_char_escaped_one_backslash() {
    assert!(is_char_escaped(b"a\\c", 2)); // 'c' escaped by single backslash
}

#[test]
fn is_char_escaped_two_backslashes() {
    // Two backslashes: first escapes second, so 'c' is NOT escaped
    assert!(!is_char_escaped(b"a\\\\c", 3));
}

#[test]
fn is_char_escaped_three_backslashes() {
    assert!(is_char_escaped(b"a\\\\\\c", 4));
}

#[test]
fn is_char_escaped_at_start() {
    assert!(!is_char_escaped(b"c", 0)); // no room for backslash
}

// ── span assertions (cannot be parameterized) ───────────────────

#[test]
fn completions_metric_from_to_range() {
    let query = "logs:http";
    let result = compute_completions(query, query.len());
    let r = result.expect("should produce completions");
    match &r {
        CompletionResult::Metric { span, .. } => {
            // `from` should point after the colon, `to` at cursor
            assert_eq!(span.from, 5); // "logs:" is 5 chars
            assert_eq!(span.to, query.len());
        }
        _ => panic!("expected Metric"),
    }
}

#[test]
fn completions_backtick_metric_from_to_range() {
    // Verify `from` points after the colon, not at some backtick boundary
    let query = "`my-ds`:met";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    match &r {
        CompletionResult::Metric { span, dataset } => {
            assert_eq!(span.from, "`my-ds`:".len()); // after the colon
            assert_eq!(span.to, query.len());
            assert_eq!(dataset, "my-ds");
        }
        _ => panic!("expected Metric, got {}", r.kind()),
    }
}

// ── single backtick dataset span assertions ─────────────────────

#[test]
fn completions_single_backtick_dataset_span() {
    // A bare backtick should produce Dataset with span.from past the backtick.
    let query = "`";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    match &r {
        CompletionResult::Dataset { span } => {
            assert_eq!(span.from, 1, "span.from should skip past opening backtick");
            assert_eq!(span.to, query.len());
        }
        _ => panic!("expected Dataset, got {}", r.kind()),
    }
}

#[test]
fn completions_single_backtick_after_space_dataset_span() {
    let query = "set x = y; `";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    match &r {
        CompletionResult::Dataset { span } => {
            assert_eq!(
                span.from,
                query.len() - "`".len() + 1,
                "span.from should skip past opening backtick"
            );
            assert_eq!(span.to, query.len());
        }
        _ => panic!("expected Dataset, got {}", r.kind()),
    }
}

// ── mid-query cursor span assertions ────────────────────────────
// Verify that span.to equals cursor, not query.len(), when text follows.

#[test]
fn mid_query_metric_span_uses_cursor() {
    let query = "logs:http | filter tag == \"x\"";
    let cursor = "logs:ht".len(); // cursor after "logs:ht"
    let r = compute_completions(query, cursor).expect("should produce metric completions");
    match &r {
        CompletionResult::Metric { span, dataset } => {
            assert_eq!(dataset, "logs");
            assert_eq!(span.from, "logs:".len());
            assert_eq!(span.to, cursor, "span.to must equal cursor, not query end");
        }
        _ => panic!("expected Metric, got {}", r.kind()),
    }
}

#[test]
fn mid_query_tag_span_uses_cursor() {
    let query = "ds:metric | filter host == \"x\"";
    let cursor = "ds:metric | filter ho".len();
    let r = compute_completions(query, cursor).expect("should produce tag completions");
    match &r {
        CompletionResult::Tag { span, .. } => {
            assert_eq!(
                span.from,
                "ds:metric | filter ".len(),
                "span.from should start at tag position"
            );
            assert_eq!(span.to, cursor, "span.to must equal cursor, not query end");
        }
        _ => panic!("expected Tag, got {}", r.kind()),
    }
}

#[test]
fn mid_query_keyword_span_uses_cursor() {
    let query = "ds:metric | filter tag == \"x\"";
    let cursor = "ds:metric | fi".len();
    let r = compute_completions(query, cursor).expect("should produce keyword completions");
    match &r {
        CompletionResult::Keywords { span, .. } => {
            assert_eq!(span.from, "ds:metric | ".len());
            assert_eq!(span.to, cursor, "span.to must equal cursor, not query end");
        }
        _ => panic!("expected Keywords, got {}", r.kind()),
    }
}

// ── negative-pattern assertions (not expressible in tables) ─────

#[test]
fn completions_source_does_not_trigger_after_pipe() {
    let query = "ds:metric | replace ";
    let result = compute_completions(query, query.len());
    assert!(
        result
            .as_ref()
            .is_none_or(|r| r.kind() != "dataset" && r.kind() != "metric"),
        "should not suggest source after a pipe"
    );
}

#[test]
fn completions_after_pipe_no_source_suggestion() {
    let query = "ds:metric | map rate-";
    let result = compute_completions(query, query.len());
    assert!(
        result
            .as_ref()
            .is_none_or(|r| r.kind() != "dataset" && r.kind() != "metric"),
        "should not suggest source after a pipe"
    );
}

#[test]
fn completions_param_space_suppresses_dataset() {
    let query = "param ";
    let result = compute_completions(query, query.len());
    assert!(
        result
            .as_ref()
            .is_none_or(|r| r.kind() != "dataset" && r.kind() != "metric"),
        "should NOT suggest dataset/metric while typing a param declaration, got {:?}",
        result.as_ref().map(|r| r.kind())
    );
}

#[test]
fn completions_set_space_suppresses_dataset() {
    let query = "set ";
    let result = compute_completions(query, query.len());
    assert!(
        result
            .as_ref()
            .is_none_or(|r| r.kind() != "dataset" && r.kind() != "metric"),
        "should NOT suggest dataset/metric while typing a set directive, got {:?}",
        result.as_ref().map(|r| r.kind())
    );
}

#[test]
fn filter_tag_position_suggests_comparison_operators() {
    let query = "ds:metric | filter tag ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    assert!(labels.contains(&"=="), "should suggest ==");
    assert!(labels.contains(&"!="), "should suggest !=");
    assert!(labels.contains(&"<"), "should suggest <");
    assert!(labels.contains(&">"), "should suggest >");
    assert!(labels.contains(&"<="), "should suggest <=");
    assert!(labels.contains(&">="), "should suggest >=");
}

/// Typing `par` AFTER query source should NOT suggest preamble keywords.
#[test]
fn completions_partial_param_after_source_no_preamble_keyword() {
    let query = "ds:metric\npar";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_ne!(
        r.kind(),
        "keywords",
        "should NOT suggest preamble keywords after query source"
    );
}

// ── apply text assertions ───────────────────────────────────────

/// Selecting a param type completion should insert the type followed by `;\n`.
#[test]
fn completions_param_type_apply_includes_semicolon_newline() {
    let query = "param $name: ";
    let r = compute_completions(query, query.len()).expect("should produce type completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    let apply_texts = r.keyword_apply_texts();
    for (label, apply) in labels.iter().zip(apply_texts.iter()) {
        let expected = format!("{label};\n");
        assert_eq!(
            *apply,
            Some(expected.as_str()),
            "param type '{label}' should have apply text '{expected}'"
        );
    }
}

// ── extract_partial_word stops at dot and hyphen ─────────────────

#[test]
fn partial_word_stops_at_dot() {
    let text = "ds.name:metric";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "name:metric");
    assert_eq!(start, "ds.".len());
}

#[test]
fn partial_word_stops_at_hyphen() {
    let text = "ds-name:metric";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "name:metric");
    assert_eq!(start, "ds-".len());
}

// ── map subtraction does not swallow hyphen ──────────────────────

#[test]
fn completions_map_subtraction_does_not_include_hyphen() {
    let query = "ds:metric | map - ";
    let (_, partial) = extract_partial_word(query, query.len());
    assert_eq!(partial, "");
}

#[test]
fn completions_map_subtraction_number() {
    let query = "ds:metric | map - 5";
    let (_, partial) = extract_partial_word(query, query.len());
    assert_eq!(partial, "5");
}

// ── function_info ────────────────────────────────────────────────

#[test]
fn function_info_known_root_function() {
    let info = STDLIB.lookup_function("avg").expect("avg should exist");
    assert_eq!(info.label, "avg");
    assert!(info.info.is_some());
}

#[test]
fn function_info_known_submodule_qualified() {
    let info = STDLIB
        .lookup_function("prom::rate")
        .expect("prom::rate should exist");
    assert_eq!(info.label, "prom::rate");
    assert!(info.info.is_some());
}

#[test]
fn function_info_unknown_returns_none() {
    assert!(STDLIB.lookup_function("nonexistent").is_none());
}

#[test]
fn function_info_unknown_qualified_returns_none() {
    assert!(STDLIB.lookup_function("fake::fn").is_none());
}

#[test]
fn function_info_bucket_function() {
    let info = STDLIB
        .lookup_function("histogram")
        .expect("histogram should exist");
    assert_eq!(info.label, "histogram");
    assert!(info.info.is_some());
}

#[test]
fn function_info_submodule_unqualified_search() {
    let info = STDLIB
        .lookup_function("linear")
        .expect("linear should be found via submodule search");
    assert_eq!(info.label, "interpolate::linear");
}

// ── regex sanity check ──────────────────────────────────────────

/// Bug 1: After `| filter ` (preceded by filter lines with regexes containing
/// pipe characters), a `Tag` completion is returned — but the span is
/// zero-length (`from == to`). The CodeMirror adapter sees an empty
/// replacement range and never opens the completion popup, even though
/// tags are fetched.
#[test]
fn completions_filter_tag_after_regex_filters() {
    let query = "\
`axiom-dev.metrics`:axiom_http_requests_total\n\
| filter path == #/.*(elastic\\/_bulk|ingest|(?:v1\\/(traces|logs|metrics))).*/\n\
| filter code == #/[123]../\n\
| filter ";
    let cursor = query.len();
    let r = compute_completions(query, cursor).expect("should produce tag completions");
    assert_eq!(r.kind(), "tag", "expected Tag completion, got {}", r.kind());
    assert_eq!(
        tag_info(&r),
        Some(("axiom-dev.metrics", "axiom_http_requests_total")),
    );
    // The span should cover the empty partial word at the cursor — the
    // adapter must accept zero-length spans to show the popup.
    match &r {
        CompletionResult::Tag { span, .. } => {
            assert_eq!(span.from, cursor, "span.from should equal cursor");
            assert_eq!(span.to, cursor, "span.to should equal cursor");
        }
        _ => unreachable!(),
    }
}

/// Bug 2: After `| group by tag1, tag2, ` the completion engine returns
/// `Keywords ["by", "using"]` instead of `Tag`. No tag fetch occurs and
/// no tag completion is offered to the user.
#[test]
fn completions_group_by_trailing_comma_suggests_tag() {
    let query = "\
`axiom-dev.metrics`:axiom_http_requests_total\n\
| group by method, path, code, ";
    let cursor = query.len();
    let r = compute_completions(query, cursor).expect("should produce completions");

    // Fixed: now correctly returns Tag for group-by label list position.
    assert_eq!(
        r.kind(),
        "tag",
        "expected Tag completion for group-by label list, got {} with labels {:?}",
        r.kind(),
        r.option_labels()
    );
    assert_eq!(
        tag_info(&r),
        Some(("axiom-dev.metrics", "axiom_http_requests_total")),
    );
}

/// After `| group by ` (just the keyword, no tags yet), should return Tag.
#[test]
fn completions_group_by_no_tags_suggests_tag() {
    let query = "ds:metric | group by ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "tag");
    assert_eq!(tag_info(&r), Some(("ds", "metric")));
}

/// After `| group by tag1, ` (single tag with trailing comma), should return Tag.
#[test]
fn completions_group_by_single_tag_trailing_comma_suggests_tag() {
    let query = "ds:metric | group by method, ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "tag");
    assert_eq!(tag_info(&r), Some(("ds", "metric")));
}

/// After `| group by a, b ` (bare tag, no trailing comma), should NOT suggest
/// more tags — the user needs to type `,` or `using` next.
#[test]
fn completions_group_by_bare_tag_suggests_using_not_tag() {
    let query = "ds:metric | group by a, b ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    assert!(labels.contains(&"using"), "should suggest 'using'");
    assert!(!labels.contains(&"by"), "should NOT suggest 'by' again");
}

/// After `| group by tag1 using `, should return GroupFunctions.
#[test]
fn completions_group_by_using_suggests_functions() {
    let query = "ds:metric | group by tag1 using ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "group_functions");
}

/// After `| group ` (no `by`), should suggest keywords ["by", "using"].
#[test]
fn completions_group_alone_suggests_keywords() {
    let query = "ds:metric | group ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    assert!(labels.contains(&"by"));
    assert!(labels.contains(&"using"));
}

// ── unclosed backtick (typing in progress) ──────────────────────

/// When typing `` `axio `` the backtick is the opening delimiter with no
/// closing pair yet. `extract_partial_word` should still include the backtick
/// so `suggest_for_source` can detect the escaped-ident context.
#[test]
fn partial_word_unclosed_backtick_dataset() {
    let text = "`axio";
    let (start, partial) = extract_partial_word(text, text.len());
    assert_eq!(start, 0);
    assert_eq!(partial, "`axio");
}

/// An unclosed backtick at the source position should produce a `Dataset`
/// completion whose span starts **after** the opening backtick, so the TS
/// adapter can filter against bare dataset names (e.g. `axio` matches
/// `axiom-dev.metrics`).
#[test]
fn completions_unclosed_backtick_dataset() {
    let query = "`axio";
    let r = compute_completions(query, query.len()).expect("should produce dataset completions");
    assert_eq!(r.kind(), "dataset");
    match &r {
        CompletionResult::Dataset { span } => {
            assert_eq!(span.from, 1, "span.from should skip the opening backtick");
            assert_eq!(span.to, query.len());
        }
        _ => unreachable!(),
    }
}

/// Unclosed backtick after a directive should still produce dataset completions.
#[test]
fn completions_unclosed_backtick_dataset_after_directive() {
    let query = "set foo = bar; `my";
    let r = compute_completions(query, query.len()).expect("should produce dataset completions");
    assert_eq!(r.kind(), "dataset");
    match &r {
        CompletionResult::Dataset { span } => {
            // "set foo = bar; `" = 16 bytes; span.from should skip the backtick
            assert_eq!(
                span.from,
                "set foo = bar; `".len(),
                "span.from should skip the opening backtick"
            );
        }
        _ => unreachable!(),
    }
}

/// Unclosed backtick inside a compute subquery source position.
#[test]
fn completions_unclosed_backtick_dataset_in_compute() {
    let query = "( `my-d";
    let r = compute_completions(query, query.len()).expect("should produce dataset completions");
    assert_eq!(r.kind(), "dataset");
    match &r {
        CompletionResult::Dataset { span } => {
            // "( `" = 3 bytes; span.from should skip the backtick to point at "my-d"
            assert_eq!(
                span.from,
                "( `".len(),
                "span.from should skip the opening backtick"
            );
        }
        _ => unreachable!(),
    }
}

/// After typing `` `my-dataset`: `` with a closed backtick dataset and colon,
/// metric completions should work and the dataset name should be unescaped.
#[test]
fn completions_closed_backtick_dataset_unclosed_backtick_metric() {
    let query = "`my-dataset`:`my-met";
    let r = compute_completions(query, query.len()).expect("should produce metric completions");
    assert_eq!(r.kind(), "metric");
    assert_eq!(source_dataset(&r), Some("my-dataset"));
    match &r {
        CompletionResult::Metric { span, .. } => {
            assert_eq!(
                span.from,
                "`my-dataset`:".len() + 1,
                "span.from should skip the opening backtick of the metric"
            );
            assert_eq!(span.to, query.len());
        }
        _ => unreachable!(),
    }
}

/// Tag completions should extract dataset/metric from escaped source
/// even when filter tag itself uses a backtick.
#[test]
fn completions_escaped_source_with_escaped_filter_tag() {
    let query = "`my-dataset`:`my-metric` | filter `my-t";
    let r = compute_completions(query, query.len()).expect("should produce tag completions");
    assert_eq!(r.kind(), "tag");
    assert_eq!(tag_info(&r), Some(("my-dataset", "my-metric")));
}

/// Group by with an escaped tag should still suggest more tags after a comma.
#[test]
fn completions_group_by_escaped_tag_trailing_comma() {
    let query = "`my-ds`:metric | group by `my-tag`, ";
    let r = compute_completions(query, query.len()).expect("should produce tag completions");
    assert_eq!(r.kind(), "tag");
    assert_eq!(tag_info(&r), Some(("my-ds", "metric")));
}

/// The empty unclosed backtick (just `` ` ``) should produce dataset completions.
#[test]
fn completions_lone_backtick_suggests_dataset() {
    let query = "`";
    let r = compute_completions(query, query.len()).expect("should produce dataset completions");
    assert_eq!(r.kind(), "dataset");
    match &r {
        CompletionResult::Dataset { span } => {
            assert_eq!(span.from, 1, "span.from should skip the backtick");
            assert_eq!(span.to, 1);
        }
        _ => unreachable!(),
    }
}

/// Unclosed backtick metric: `` `ds`:`met `` — the metric part has an
/// unclosed backtick, so the span should skip it.
#[test]
fn completions_unclosed_backtick_metric_span() {
    let query = "ds:`met";
    let r = compute_completions(query, query.len()).expect("should produce metric completions");
    assert_eq!(r.kind(), "metric");
    assert_eq!(source_dataset(&r), Some("ds"));
    match &r {
        CompletionResult::Metric { span, .. } => {
            assert_eq!(
                span.from,
                "ds:`".len(),
                "span.from should skip the opening backtick of the metric"
            );
            assert_eq!(span.to, query.len());
        }
        _ => unreachable!(),
    }
}

// ── unclosed backtick tag after regex filters ───────────────────

/// Bug: after filter lines containing regex patterns with special characters,
/// typing a backtick-escaped tag name (`` `l ``) fails to produce tag
/// completions. `extract_partial_word` incorrectly pairs the unclosed opening
/// backtick with the closing backtick of the escaped dataset name, producing
/// a partial word spanning the entire query.
#[test]
fn completions_backtick_tag_after_regex_filters() {
    let query = "\
`axiom-dev.metrics`:axiomdb_transport_request_duration_seconds_bucket\n\
| filter path == #/.*(elastic\\/_bulk|ingest|(?:v1\\/(traces|logs|metrics))).*/\n\
| filter code == #/[123]../\n\
| filter `l";
    let cursor = query.len();
    let r = compute_completions(query, cursor).expect("should produce tag completions");
    assert_eq!(r.kind(), "tag", "expected Tag completion, got {}", r.kind());
    assert_eq!(
        tag_info(&r),
        Some((
            "axiom-dev.metrics",
            "axiomdb_transport_request_duration_seconds_bucket"
        )),
    );
}

/// Same bug but with a simpler reproduction: escaped source + any intervening
/// content + unclosed backtick tag.
#[test]
fn completions_backtick_tag_after_closed_backtick_source() {
    let query = "`my-ds`:`my-metric` | filter `t";
    let cursor = query.len();
    let r = compute_completions(query, cursor).expect("should produce tag completions");
    assert_eq!(r.kind(), "tag", "expected Tag completion, got {}", r.kind());
    assert_eq!(tag_info(&r), Some(("my-ds", "my-metric")));
}

/// `extract_partial_word` must not cross whitespace when scanning for a
/// matching backtick. The unclosed backtick in `` `l `` should not pair with
/// the closing backtick of a previous escaped identifier separated by spaces.
#[test]
fn partial_word_unclosed_backtick_after_closed_pair() {
    let text = "`my-ds`:`my-metric` | filter `l";
    let (start, partial) = extract_partial_word(text, text.len());
    // The partial should be "`l" — just the unclosed backtick + typed char
    assert_eq!(partial, "`l");
    assert_eq!(start, text.len() - 2);
}

/// Variant: escaped source, multiple filter stages, then unclosed backtick tag.
#[test]
fn completions_backtick_tag_after_multiple_filters_with_escaped_source() {
    let query = "`axiom-dev.metrics`:metric | filter a == \"x\" | filter `t";
    let cursor = query.len();
    let r = compute_completions(query, cursor).expect("should produce tag completions");
    assert_eq!(r.kind(), "tag", "expected Tag completion, got {}", r.kind());
    assert_eq!(tag_info(&r), Some(("axiom-dev.metrics", "metric")),);
}

// ── filter value position (no completions expected) ─────────────

/// Bug: After `| filter tag == ` the completion engine returns
/// `Keywords ["and", "or", "not"]` instead of `None`. The cursor is at the
/// value position of a comparison — no keyword completions apply here.
#[test]
fn completions_filter_value_position_returns_none() {
    let query = "ds:metric | filter tag == ";
    let result = compute_completions(query, query.len());
    assert!(
        result.is_none(),
        "expected None at value position after ==, got {:?}",
        result.as_ref().map(|r| r.kind())
    );
}

/// Same bug with `!=` operator.
#[test]
fn completions_filter_value_position_neq_returns_none() {
    let query = "ds:metric | filter tag != ";
    let result = compute_completions(query, query.len());
    assert!(
        result.is_none(),
        "expected None at value position after !=, got {:?}",
        result.as_ref().map(|r| r.kind())
    );
}

/// Same bug with `<` operator.
#[test]
fn completions_filter_value_position_lt_returns_none() {
    let query = "ds:metric | filter tag < ";
    let result = compute_completions(query, query.len());
    assert!(
        result.is_none(),
        "expected None at value position after <, got {:?}",
        result.as_ref().map(|r| r.kind())
    );
}

/// Same bug with `>=` operator.
#[test]
fn completions_filter_value_position_gte_returns_none() {
    let query = "ds:metric | filter tag >= ";
    let result = compute_completions(query, query.len());
    assert!(
        result.is_none(),
        "expected None at value position after >=, got {:?}",
        result.as_ref().map(|r| r.kind())
    );
}

/// Exact reproduction of the reported bug: escaped tag after regex filters.
#[test]
fn completions_filter_escaped_tag_value_position_returns_none() {
    let query = "\
`axiom-dev.metrics`:axiom_http_requests_total\n\
| filter path == #/.*(elastic\\/_bulk|ingest|(?:v1\\/(traces|logs|metrics))).*/\n\
| filter code == #/[123]../\n\
| filter `container` == ";
    let result = compute_completions(query, query.len());
    assert!(
        result.is_none(),
        "expected None at value position after `container` ==, got {:?}",
        result.as_ref().map(|r| r.kind())
    );
}

/// After a completed filter atom (`tag == "val" `), and/or/not ARE correct.
#[test]
fn completions_filter_after_completed_atom_suggests_boolean_ops() {
    let query = "ds:metric | filter tag == \"value\" ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    assert!(labels.contains(&"and"), "should suggest 'and'");
    assert!(labels.contains(&"or"), "should suggest 'or'");
    assert!(labels.contains(&"not"), "should suggest 'not'");
}

/// After a completed regex filter (`tag == #/re/ `), and/or/not ARE correct.
#[test]
fn completions_filter_after_completed_regex_suggests_boolean_ops() {
    let query = "ds:metric | filter tag == #/abc/ ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    assert!(labels.contains(&"and"), "should suggest 'and'");
    assert!(labels.contains(&"or"), "should suggest 'or'");
}

/// After a completed numeric filter, and/or/not ARE correct.
#[test]
fn completions_filter_after_completed_number_suggests_boolean_ops() {
    let query = "ds:metric | filter tag > 42 ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    assert!(labels.contains(&"and"), "should suggest 'and'");
}

/// After a completed bool filter, and/or/not ARE correct.
#[test]
fn completions_filter_after_completed_bool_suggests_boolean_ops() {
    let query = "ds:metric | filter tag == true ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    assert!(labels.contains(&"and"), "should suggest 'and'");
}

// Regexes are unanchored (substring match) by default.
#[test]
fn unanchored_regexp() {
    let r = Regex::new("bc").expect("bad regexp");
    assert!(r.is_match("bc"));
    assert!(r.is_match("abc"));
    assert!(r.is_match("bcd"));
}

// ── extract_declared_params ─────────────────────────────────────

#[test]
fn extract_params_single() {
    let params = extract_declared_params("param $interval: duration;\nds:metric");
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].label, "$interval");
    assert_eq!(params[0].typ, ParamType::Duration);
}

#[test]
fn extract_params_multiple() {
    let text = "param $ds: dataset;\nparam $str: string;\nparam $r: regex;\nds:metric";
    let params = extract_declared_params(text);
    assert_eq!(params.len(), 3);
    assert_eq!(params[0].label, "$ds");
    assert_eq!(params[0].typ, ParamType::Dataset);
    assert_eq!(params[1].label, "$str");
    assert_eq!(params[1].typ, ParamType::String);
    assert_eq!(params[2].label, "$r");
    assert_eq!(params[2].typ, ParamType::Regex);
}

#[test]
fn extract_params_after_directives() {
    let text = "set foo = bar;\nparam $x: int;\nds:metric";
    let params = extract_declared_params(text);
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].label, "$x");
    assert_eq!(params[0].typ, ParamType::Int);
}

#[test]
fn extract_params_all_types() {
    let text = "\
param $a: dataset;\n\
param $b: metric;\n\
param $c: duration;\n\
param $d: string;\n\
param $e: int;\n\
param $f: float;\n\
param $g: bool;\n\
param $h: regex;\n\
ds:metric";
    let params = extract_declared_params(text);
    assert_eq!(params.len(), 8);
    let types: Vec<ParamType> = params.iter().map(|p| p.typ).collect();
    assert_eq!(
        types,
        vec![
            ParamType::Dataset,
            ParamType::Metric,
            ParamType::Duration,
            ParamType::String,
            ParamType::Int,
            ParamType::Float,
            ParamType::Bool,
            ParamType::Regex,
        ]
    );
}

#[test]
fn extract_params_none_when_no_params() {
    let params = extract_declared_params("ds:metric | filter tag == \"val\"");
    assert!(params.is_empty());
}

#[test]
fn extract_params_with_comments() {
    let text = "// header comment\nparam $x: string;\n// another comment\nds:metric";
    let params = extract_declared_params(text);
    assert_eq!(params.len(), 1);
    assert_eq!(params[0].label, "$x");
}

// ── extract_partial_word includes $ ─────────────────────────────

#[test]
fn partial_word_dollar_sign() {
    let text = "ds:metric | filter tag == $fo";
    let (_, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "$fo");
}

#[test]
fn partial_word_dollar_only() {
    let text = "ds:metric | filter tag == $";
    let (_, partial) = extract_partial_word(text, text.len());
    assert_eq!(partial, "$");
}

/// After `| filter path == ` preceded by filter lines with regex patterns
/// containing pipe characters, a string param should be suggested.
#[test]
fn completions_filter_value_param_after_regex_filters() {
    let query = "param $str: string;\n\
\n\
`axiom-dev.metrics`:axiom_http_requests_total\n\
| filter path == #/.*(elastic\\/_bulk|ingest|(?:v1\\/(traces|logs|metrics))).*/\n\
| filter code == #/[123]../\n\
| filter path == ";
    let cursor = query.len();
    let r = compute_completions(query, cursor).expect("should produce param completions");
    assert_eq!(
        r.kind(),
        "params",
        "expected params completion, got {} with labels {:?}",
        r.kind(),
        r.option_labels()
    );
    let labels = r.option_labels();
    assert!(labels.contains(&"$str"), "should suggest $str param");
}

// ── cumulative histogram (contain + exclude in same test) ───────

#[test]
fn completions_bucket_cumulative_first_arg() {
    let query = "ds:metric | bucket to 1m using interpolate_cumulative_histogram(";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    assert!(labels.contains(&"rate"));
    assert!(labels.contains(&"increase"));
    assert!(
        !labels.contains(&"count"),
        "should not suggest bucket specs for first arg"
    );
}

#[test]
fn completions_bucket_cumulative_after_conversion() {
    let query = "ds:metric | bucket to 1m using interpolate_cumulative_histogram(rate, ";
    let r = compute_completions(query, query.len()).expect("should produce completions");
    assert_eq!(r.kind(), "keywords");
    let labels = r.option_labels();
    assert!(labels.contains(&"count"));
    assert!(labels.contains(&"avg"));
    assert!(
        !labels.contains(&"rate"),
        "should not suggest conversions after first comma"
    );
}

// ═══════════════════════════════════════════════════════════════════
// Parameterized completion tests
//
// Input uses `#` to mark cursor position. The marker is stripped and
// `compute_completions` is called with the resulting text and offset.
//
// Convention:
//   - `test_completion_tag_info` and `test_completion_source_dataset`
//     also assert `kind()` ("tag" / "metric" respectively).
//   - `test_completion_params_contain` also asserts `kind() == "params"`.
//   - `test_completion_labels_*` do NOT assert kind — pair with a
//     `test_completion_kind` row when kind coverage is needed.
// ═══════════════════════════════════════════════════════════════════

/// Parses `#` as the cursor position, removes it, and calls
/// `compute_completions` with the remaining text and cursor offset.
fn completions_at(input: &str) -> Option<CompletionResult> {
    let cursor = input
        .find('#')
        .expect("input must contain a # cursor marker");
    let query = format!("{}{}", &input[..cursor], &input[cursor + 1..]);
    compute_completions(&query, cursor)
}

// ── completion kind ─────────────────────────────────────────────

#[test_case("#"                                          => Some("dataset")          ; "empty query suggests dataset")]
#[test_case("lo#"                                        => Some("dataset")          ; "partial dataset name")]
#[test_case("logs:#"                                     => Some("metric")           ; "colon after dataset suggests metric")]
#[test_case("logs:http_req#"                             => Some("metric")           ; "partial metric name")]
#[test_case("set foo = bar; lo#"                         => Some("dataset")          ; "dataset after directive")]
#[test_case("ds:metric | #"                              => Some("keywords")         ; "pipe position suggests keywords")]
#[test_case("ds:metric | filter #"                       => Some("tag")              ; "filter suggests tag")]
// ── dataset given, no metric (with colon) ───────────────────────
#[test_case("ds:#"                                       => Some("metric")           ; "dataset colon no metric suggests metric")]
#[test_case("`my-dataset`:#"                             => Some("metric")           ; "backtick dataset colon no metric suggests metric")]
// ── dataset given, no metric (without colon) ────────────────────
#[test_case("ds | #"                                     => Some("keywords")         ; "dataset no colon pipe suggests keywords")]
#[test_case("ds | filter #"                              => None                     ; "dataset no colon filter tag returns none")]
#[test_case("ds | where #"                               => None                     ; "dataset no colon where tag returns none")]
#[test_case("ds | filter tag == \"x\" #"                 => Some("keywords")         ; "dataset no colon after completed filter")]
#[test_case("ds | filter tag #"                          => Some("keywords")         ; "dataset no colon filter tag comparison ops")]
#[test_case("ds | map #"                                 => Some("map_functions")    ; "dataset no colon map suggests functions")]
#[test_case("ds | align to 1m using #"                   => Some("align_functions")  ; "dataset no colon align using suggests functions")]
#[test_case("ds | group by #"                            => None                     ; "dataset no colon group by tag returns none")]
#[test_case("ds | group using #"                         => Some("group_functions")  ; "dataset no colon group using suggests functions")]
#[test_case("`my-dataset` | filter #"                    => None                     ; "backtick dataset no colon filter returns none")]
#[test_case("`my-dataset` | where tag == \"x\" #"        => Some("keywords")         ; "backtick dataset no colon after completed filter")]
#[test_case("ds:metric | filter tag == #"                => None                     ; "filter value position no params")]
#[test_case("ds:metric | filter tag != #"                => None                     ; "filter value neq no params")]
#[test_case("ds:metric | filter tag < #"                 => None                     ; "filter value lt no params")]
#[test_case("ds:metric | filter tag >= #"                => None                     ; "filter value gte no params")]
#[test_case("ds:metric | filter tag == \"value\" #"      => Some("keywords")         ; "after completed string filter")]
#[test_case("ds:metric | filter tag == /abc/ #"          => Some("keywords")         ; "after completed regex filter")]
#[test_case("ds:metric | filter tag > 42 #"              => Some("keywords")         ; "after completed numeric filter")]
#[test_case("ds:metric | filter tag == true #"           => Some("keywords")         ; "after completed bool filter")]
#[test_case("ds:metric | group #"                        => Some("keywords")         ; "group alone suggests keywords")]
#[test_case("ds:metric | group by #"                     => Some("tag")              ; "group by suggests tag")]
#[test_case("ds:metric | group by method, #"             => Some("tag")              ; "group by trailing comma suggests tag")]
#[test_case("ds:metric | group by a, b #"                => Some("keywords")         ; "group by bare tag suggests keywords")]
#[test_case("ds:metric | group by tag1 using #"          => Some("group_functions")  ; "group by using suggests functions")]
#[test_case("( lo#"                                      => Some("dataset")          ; "compute subquery dataset")]
#[test_case("( ds:#"                                     => Some("metric")           ; "compute subquery metric")]
#[test_case("( ds:metric | filter #"                     => Some("tag")              ; "compute subquery filter tag")]
#[test_case("( ds1:m1 , ds2:m2 | filter #"               => Some("tag")              ; "compute second subquery tag")]
#[test_case("( ds1:m1 , ds2:m2 ) | #"                    => Some("keywords")         ; "compute outer pipe")]
#[test_case("( ds1:m1 , ds2:m2 ) | com#"                 => Some("keywords")         ; "compute partial word")]
#[test_case("( ds1:m1 , ds2:m2 ) | compute result #"     => Some("keywords")         ; "compute name suggests using")]
#[test_case("( ds1:m1 , ds2:m2 ) | compute result using #" => Some("compute_functions") ; "compute using suggests functions")]
#[test_case("( ds1:m1 , ds2:m2 ) | compute result using / | #" => Some("keywords")  ; "compute tail pipe")]
#[test_case("( ( a:b | filter #"                         => Some("tag")              ; "nested compute inner tag")]
#[test_case("par#"                                       => Some("keywords")         ; "partial preamble param")]
#[test_case("se#"                                        => Some("keywords")         ; "partial preamble set")]
#[test_case("param $name: #"                             => Some("keywords")         ; "param type after colon")]
#[test_case("param $name: du#"                           => Some("keywords")         ; "param type partial")]
#[test_case("`#"                                         => Some("dataset")          ; "single backtick suggests dataset")]
#[test_case("set foo = bar; `#"                          => Some("dataset")          ; "single backtick after directive suggests dataset")]
#[test_case("( `#"                                       => Some("dataset")          ; "single backtick in compute subquery suggests dataset")]
#[test_case("`my-data#"                                  => Some("dataset")          ; "unclosed backtick dataset")]
#[test_case("`my-dataset`:met#"                          => Some("metric")           ; "backtick dataset partial metric")]
#[test_case("`my-dataset`:`my-met#"                      => Some("metric")           ; "both backticked partial metric")]
#[test_case("`my-dataset`:`my-metric` | #"               => Some("keywords")         ; "backtick pipe keywords")]
#[test_case("param $dataset: dataset;\n#"                => None                     ; "no dataset after param decl")]
#[test_case("param $ds: dataset;\nparam $m: metric;\n#"  => None                     ; "no dataset after multiple params")]
#[test_case("set foo = bar;\nparam $ds: dataset;\n#"     => None                     ; "no dataset after directive and params")]
#[test_case("ds:metric | bucket to 1m using histogram(#" => Some("keywords")         ; "bucket histogram open paren")]
#[test_case("ds:metric | bucket to 1m using histogram(co#" => Some("keywords")       ; "bucket histogram partial")]
#[test_case("ds:metric | bucket to 1m using histogram(count, #" => Some("keywords")  ; "bucket histogram after comma")]
#[test_case("ds:metric | bucket to 1m using interpolate_delta_histogram(#" => Some("keywords") ; "bucket interpolate delta histogram")]
#[test_case("ds:metric | bucket to 1m using histogram(count) | #" => Some("keywords"); "after bucket closed paren")]
#[test_case("param $ds: dataset;\nlo#"                   => Some("dataset")          ; "plain text source with params")]
#[test_case("param $m: metric;\nds:met#"                 => Some("metric")           ; "plain metric source with params")]
#[test_case("param $foo: string;\nparam $bar: int;\nds:metric | filter tag == $fo#" => Some("params") ; "partial param at filter value")]
#[test_case("param $ds: dataset;\n$#"                    => Some("params")           ; "dollar at dataset position")]
#[test_case("param $ds: dataset;\nparam $other: string;\n$d#" => Some("params")      ; "partial dollar at dataset position")]
#[test_case("param $m: metric;\nds:$#"                   => Some("params")           ; "dollar at metric position")]
#[test_case("param $w: duration;\nds:metric | align to #" => Some("params")          ; "align to with duration param")]
#[test_case("param $w: duration;\nds:metric | align to 1m over #" => Some("params")  ; "align to over with duration param")]
#[test_case("param $w: duration;\nds:metric | bucket to #" => Some("params")         ; "bucket to with duration param")]
#[test_case("param $s: string;\nds:metric | align to #"  => None                     ; "align to no duration params")]
#[test_case("ds:metric | filter tag == #"                => None                     ; "filter value no params returns none")]
#[test_case("ds:metric | where tag == #"                 => None                     ; "where value no params returns none")]
#[test_case("`axiom-dev.metrics`:axiom_http_requests_total\n| align #" => Some("keywords") ; "align initial kind")]
#[test_case("`axiom-dev.metrics`:axiom_http_requests_total\n| align to 42s #" => Some("keywords") ; "align after to kind")]
#[test_case("ds:metric | align to 42s over 1h #"        => Some("keywords")         ; "align after to over kind")]
// ── mid-query cursor: text after cursor must not affect kind ────
#[test_case("lo#gs:http"                                  => Some("dataset")          ; "mid dataset suffix ignored")]
#[test_case("logs:ht#tp | filter tag == \"x\""            => Some("metric")           ; "mid metric trailing pipe ignored")]
#[test_case("ds:metric | filter ho#st == \"x\""           => Some("tag")              ; "mid filter tag suffix ignored")]
#[test_case("ds:metric | group by ho#st using sum"        => Some("tag")              ; "mid group-by tag trailing using ignored")]
#[test_case("ds:metric | #filter tag == \"x\""            => Some("keywords")         ; "cursor before filter keyword")]
#[test_case("ds:metric | wh#ere tag == \"x\""            => Some("keywords")         ; "mid filter keyword suffix ignored")]
#[test_case("( a:b | wh#ere x == \"v\" , c:d ) | compute out using /" => Some("keywords") ; "mid compute subquery keyword")]
#[test_case("( a:b , c:d ) | com#pute out using / | map rate" => Some("keywords")     ; "mid compute rule keyword tail ignored")]
#[test_case("ds:metric | bucket to 1m using interpolate_cumulative_histogram(ra#te, count)" => Some("keywords") ; "mid bucket arg suffix ignored")]
#[test_case("#ds:metric | filter tag == \"v\""            => Some("dataset")          ; "cursor at start of query")]
#[test_case("param $s: string;\nds:metric | filter tag == $s#tr and other == 1" => Some("params") ; "mid param value suffix ignored")]
#[test_case("ds:metric | filter tag #"                    => Some("keywords")         ; "filter tag suggests comparison operators")]
#[test_case("ds:metric | where tag #"                     => Some("keywords")         ; "where tag suggests comparison operators")]
#[test_case("ds:metric | filter (#"                       => Some("tag")              ; "open paren after filter suggests tag")]
#[test_case("ds:metric | where (#"                        => Some("tag")              ; "open paren after where suggests tag")]
#[test_case("ds:metric | where tag is #"                  => Some("keywords")         ; "where tag is suggests type keywords")]
#[test_case("ds:metric | filter tag is #"                 => Some("keywords")         ; "filter tag is suggests type keywords")]
#[test_case("ds:metric | where tag is string #"           => Some("keywords")         ; "after is string suggests boolean ops")]
#[test_case("ds:metric | filter tag is bool #"            => Some("keywords")         ; "after is bool suggests boolean ops")]
#[test_case("ds:metric | sample #"                        => None                     ; "sample takes number no completions")]
fn test_completion_kind(input: &str) -> Option<&'static str> {
    completions_at(input).map(|r| r.kind())
}

// ── labels contain ──────────────────────────────────────────────

#[test_case("ds:metric | #",                      &["sample", "where", "map", "align", "group", "bucket"]   ; "pipe keywords")]
#[test_case("ds | #",                             &["sample", "where", "map", "align", "group", "bucket"]   ; "no metric pipe keywords")]
#[test_case("ds | filter tag == \"v\" #",          &["and", "or"]                                  ; "no metric boolean ops after string")]
#[test_case("ds:metric | where tag == \"v\" #",   &["and", "or"]                           ; "boolean ops after string")]
#[test_case("ds:metric | where tag == /re/ #",    &["and", "or"]                                  ; "boolean ops after regex")]
#[test_case("ds:metric | where tag > 42 #",       &["and", "or"]                                        ; "boolean ops after number")]
#[test_case("ds:metric | where tag == true #",    &["and", "or"]                                        ; "boolean ops after bool")]
#[test_case("ds:metric | group #",                 &["by", "using"]                                ; "group sub-keywords")]
#[test_case("ds:metric | group by a, b #",         &["using"]                                      ; "group by bare tag has using")]
#[test_case("par#",                                &["param"]                                       ; "partial param keyword")]
#[test_case("se#",                                 &["set"]                                         ; "partial set keyword")]
#[test_case("ds:metric | bucket to 1m using histogram(#", &["count", "avg", "sum", "min", "max"]   ; "bucket spec keywords")]
#[test_case("ds:metric | bucket to 1m using histogram(count, #", &["avg", "sum"]                   ; "bucket histogram after comma")]
#[test_case("ds:metric | bucket to 1m using interpolate_delta_histogram(#", &["count", "avg"]      ; "interpolate delta histogram specs")]
#[test_case("ds:metric | bucket to 1m using histogram(count) | #", &["where"]                     ; "after bucket closed paren has filter")]
#[test_case("( ds1:m1 , ds2:m2 ) | compute result using #", &["/", "+"]                            ; "compute operators")]
#[test_case("( ds1:m1 , ds2:m2 ) | compute result using / | #", &["map", "align"]                  ; "compute tail has map and align")]
#[test_case("`axiom-dev.metrics`:axiom_http_requests_total\n| align #", &["to"]                    ; "align initially suggests to")]
#[test_case("`axiom-dev.metrics`:axiom_http_requests_total\n| align to 42s #", &["using"]          ; "align after to suggests using")]
#[test_case("param $name: #", &["dataset", "metric", "duration", "string", "int", "float", "bool", "regex"] ; "all param types")]
#[test_case("`my-dataset`:`my-metric` | #", &["sample", "where", "map"]                           ; "backtick pipe keywords")]
// ── mid-query cursor ────────────────────────────────────────────
#[test_case("ds:metric | wh#ere tag == \"x\"",   &["where"]                                       ; "mid keyword contains where")]
#[test_case("( a:b , c:d ) | com#pute out using / | map rate", &["compute"]                        ; "mid compute rule contains compute")]
#[test_case("ds:metric | bucket to 1m using interpolate_cumulative_histogram(ra#te, count)", &["rate"] ; "mid bucket arg contains rate")]
#[test_case("ds:metric | filter tag #",                    &["is"]                    ; "filter tag includes is operator")]
#[test_case("ds:metric | where tag #",                     &["is"]                    ; "where tag includes is operator")]
#[test_case("ds:metric | where tag is #",                  &["string", "int", "float", "bool"] ; "is suggests all tag types")]
#[test_case("ds:metric | where tag is string #",           &["and", "or", "not"]      ; "after is string suggests boolean ops")]
#[test_case("ds:metric | filter tag is bool #",            &["and", "or"]             ; "after is bool suggests boolean ops")]
fn test_completion_labels_contain(input: &str, expected: &[&str]) {
    let r = completions_at(input).expect("should produce completions");
    let labels = r.option_labels();
    for exp in expected {
        assert!(labels.contains(exp), "expected label '{exp}' in {labels:?}");
    }
}

// ── labels exclude ──────────────────────────────────────────────

#[test_case("ds:metric | sample 0.1 | #",                                          &["sample"]           ; "second pipe excludes sample")]
#[test_case("ds:metric | where tag == \"v\" | #",                                  &["sample"]           ; "after filter excludes sample")]
#[test_case("( ds1:m1 , ds2:m2 ) | compute result using / | #",                    &["sample"]           ; "compute tail excludes sample")]
#[test_case("( ds1:m1 , ds2:m2 ) | compute result using / | #", &["where", "filter"] ; "compute tail excludes filter")]
#[test_case("`axiom-dev.metrics`:axiom_http_requests_total\n| align #", &["using"]    ; "align initially excludes using")]
#[test_case("`axiom-dev.metrics`:axiom_http_requests_total\n| align to 42s #", &["to"]; "align after to excludes to")]
#[test_case("ds:metric | group by a, b #", &["by"]                                    ; "group by bare tag excludes by")]
// ── mid-query cursor ────────────────────────────────────────────
#[test_case("ds:metric | bucket to 1m using interpolate_cumulative_histogram(ra#te, count)", &["count"] ; "mid bucket first arg excludes specs")]
#[test_case("ds:metric | where tag is #",                  &["dataset", "metric", "duration", "regex"] ; "is excludes non-tag types")]
fn test_completion_labels_exclude(input: &str, excluded: &[&str]) {
    let r = completions_at(input).expect("should produce completions");
    let labels = r.option_labels();
    for excl in excluded {
        assert!(
            !labels.contains(excl),
            "label '{excl}' should NOT be in {labels:?}"
        );
    }
}

// ── exact labels ────────────────────────────────────────────────

#[test_case("( ds1:m1 , ds2:m2 ) | #",              &["compute"]  ; "compute outer exact")]
#[test_case("( ds1:m1 , ds2:m2 ) | com#",           &["compute"]  ; "compute partial filters")]
#[test_case("( ds1:m1 , ds2:m2 ) | compute result #", &["using"]  ; "compute name only using")]
#[test_case("ds:metric | align to 42s over 1h #",   &["using"]    ; "align after to over only using")]
#[test_case("ds:metric | bucket to 1m using histogram(co#", &["count"] ; "histogram partial to count")]
#[test_case("param $name: du#",                     &["duration"] ; "param type du to duration")]
fn test_completion_labels_exact(input: &str, expected: &[&str]) {
    let r = completions_at(input).expect("should produce completions");
    let labels = r.option_labels();
    assert_eq!(labels, expected);
}

// ── tag info ────────────────────────────────────────────────────

#[test_case("ds:metric | filter #",                                   "ds",                "metric"   ; "simple filter")]
#[test_case("`my-dataset`:metric | filter #",                         "my-dataset",        "metric"   ; "backtick dataset filter")]
#[test_case("`my-dataset`:`my-metric` | filter `my-t#",               "my-dataset",        "my-metric"; "escaped source escaped tag")]
#[test_case("( ds:metric | filter #",                                 "ds",                "metric"   ; "compute subquery")]
#[test_case("( ds1:m1 , ds2:m2 | filter #",                           "ds2",               "m2"       ; "compute second subquery")]
#[test_case("( ( a:b | filter #",                                     "a",                 "b"        ; "nested compute inner")]
#[test_case("ds:metric | group by #",                                 "ds",                "metric"   ; "group by")]
#[test_case("ds:metric | group by method, #",                         "ds",                "metric"   ; "group by trailing comma")]
#[test_case("`my-ds`:metric | group by `my-tag`, #",                  "my-ds",             "metric"   ; "escaped group by comma")]
#[test_case("`my-dataset`:metric | filter `my-t#",                    "my-dataset",        "metric"   ; "backtick tag partial")]
#[test_case("`my-ds`:`my-metric` | filter `t#",                       "my-ds",             "my-metric"; "backtick tag after closed source")]
#[test_case("`axiom-dev.metrics`:metric | filter a == \"x\" | filter `t#", "axiom-dev.metrics", "metric" ; "backtick tag after multiple filters")]
#[test_case("`axiom-dev.metrics`:axiom_http_requests_total\n| group by method, path, code, #", "axiom-dev.metrics", "axiom_http_requests_total" ; "group by multiline trailing comma")]
// ── mid-query cursor ────────────────────────────────────────────
#[test_case("ds:metric | filter ho#st == \"x\"",                       "ds",     "metric"   ; "mid filter tag suffix ignored")]
#[test_case("ds:metric | group by ho#st using sum",                    "ds",     "metric"   ; "mid group-by tag trailing using ignored")]
fn test_completion_tag_info(input: &str, expected_ds: &str, expected_metric: &str) {
    let r = completions_at(input).expect("should produce completions");
    assert_eq!(r.kind(), "tag");
    assert_eq!(tag_info(&r), Some((expected_ds, expected_metric)));
}

// ── source dataset ──────────────────────────────────────────────

#[test_case("ds:#",                      "ds"         ; "dataset colon no metric")]
#[test_case("`my-dataset`:#",            "my-dataset" ; "backtick dataset colon no metric")]
#[test_case("logs:#",                    "logs"       ; "simple")]
#[test_case("logs:http_req#",            "logs"       ; "partial metric")]
#[test_case("`my-dataset`:met#",         "my-dataset" ; "backtick dataset partial metric")]
#[test_case("`my-ds`:`my-met`#",         "my-ds"      ; "both backticked")]
#[test_case("( ds:#",                    "ds"         ; "compute subquery")]
#[test_case("( `my-ds`:#",               "my-ds"      ; "compute subquery backtick")]
#[test_case("set foo = bar; `my-ds`:m#", "my-ds"      ; "after directive backtick")]
// ── mid-query cursor ────────────────────────────────────────────
#[test_case("logs:ht#tp | filter tag == \"x\"", "logs" ; "mid metric trailing pipe ignored")]
fn test_completion_source_dataset(input: &str, expected: &str) {
    let r = completions_at(input).expect("should produce completions");
    assert_eq!(r.kind(), "metric");
    assert_eq!(source_dataset(&r), Some(expected));
}

// ── param labels contain ────────────────────────────────────────

#[test_case("param $s: string;\nparam $i: int;\nds:metric | where tag == #", &["$s", "$i"] ; "where value params")]
#[test_case("param $r: regex;\nparam $s: string;\nds:metric | where tag == #", &["$r", "$s"]; "eq includes regex")]
#[test_case("param $r: regex;\nds:metric | where tag != #", &["$r"]                         ; "neq includes regex")]
#[test_case("param $r: regex;\nparam $i: int;\nds:metric | filter tag < #", &["$i"]          ; "lt includes int")]
#[test_case("param $r: regex;\nparam $f: float;\nds:metric | filter tag >= #", &["$f"]       ; "gte includes float")]
#[test_case("param $d: dataset;\nparam $m: metric;\nparam $s: string;\nds:metric | filter tag == #", &["$s"] ; "filter value includes string")]
#[test_case("param $dur: duration;\nparam $b: bool;\nds:metric | filter tag == #", &["$b"]   ; "filter value includes bool")]
#[test_case("param $foo: string;\nparam $bar: int;\nds:metric | filter tag == $fo#", &["$foo"] ; "partial param filtered")]
#[test_case("param $w: duration;\nds:metric | align to #", &["$w"]                           ; "align to duration")]
#[test_case("param $w: duration;\nds:metric | align to 1m over #", &["$w"]                   ; "align over duration")]
#[test_case("param $w: duration;\nds:metric | bucket to #", &["$w"]                          ; "bucket to duration")]
#[test_case("param $s: string;\nparam $w: duration;\nds:metric | align to #", &["$w"]        ; "align to includes duration")]
#[test_case("param $ds: dataset;\n$#", &["$ds"]                                              ; "source dataset param")]
#[test_case("param $ds: dataset;\nparam $other: string;\n$d#", &["$ds"]                      ; "source dataset param partial")]
#[test_case("param $s: string;\nparam $d: dataset;\n$#", &["$d"]                             ; "source dataset includes dataset")]
#[test_case("param $m: metric;\nds:$#", &["$m"]                                              ; "source metric param")]
#[test_case("param $s: string;\nparam $m: metric;\nds:$#", &["$m"]                           ; "source metric includes metric")]
// ── mid-query cursor ────────────────────────────────────────────
#[test_case("param $str: string;\nds:metric | filter tag == $s#tr and other == 1", &["$str"] ; "mid param suffix ignored")]
fn test_completion_params_contain(input: &str, expected: &[&str]) {
    let r = completions_at(input).expect("should produce param completions");
    assert_eq!(r.kind(), "params");
    let labels = r.option_labels();
    for exp in expected {
        assert!(labels.contains(exp), "expected param '{exp}' in {labels:?}");
    }
}

// ── param labels exclude ────────────────────────────────────────

#[test_case("param $r: regex;\nparam $i: int;\nds:metric | filter tag < #",   &["$r"]  ; "lt excludes regex")]
#[test_case("param $r: regex;\nparam $f: float;\nds:metric | where tag >= #", &["$r"]  ; "gte excludes regex")]
#[test_case("param $d: dataset;\nparam $s: string;\nds:metric | where tag == #", &["$d"] ; "filter excludes dataset")]
#[test_case("param $d: dataset;\nparam $m: metric;\nparam $s: string;\nds:metric | filter tag == #", &["$d", "$m"] ; "filter excludes dataset and metric")]
#[test_case("param $dur: duration;\nparam $b: bool;\nds:metric | filter tag == #", &["$dur"] ; "filter excludes duration")]
#[test_case("param $foo: string;\nparam $bar: int;\nds:metric | filter tag == $fo#", &["$bar"] ; "partial param excludes non-matching")]
#[test_case("param $s: string;\nparam $w: duration;\nds:metric | align to #", &["$s"]   ; "align excludes non-duration")]
#[test_case("param $s: string;\nparam $d: dataset;\n$#", &["$s"]                        ; "source excludes non-dataset")]
#[test_case("param $ds: dataset;\nparam $other: string;\n$d#", &["$other"]               ; "source partial excludes non-matching")]
#[test_case("param $s: string;\nparam $m: metric;\nds:$#", &["$s"]                      ; "metric excludes non-metric")]
fn test_completion_params_exclude(input: &str, excluded: &[&str]) {
    let r = completions_at(input).expect("should produce param completions");
    let labels = r.option_labels();
    for excl in excluded {
        assert!(
            !labels.contains(excl),
            "param '{excl}' should NOT be in {labels:?}"
        );
    }
}
