use crate::wasm::diagnostics::{DiagnosticItem, Severity, maybe_rewrite_escaped_dataset_error};
use crate::{CompileError, compile};

fn diagnostic_items(q: &str) -> Vec<DiagnosticItem> {
    match compile(q) {
        Ok(_) => vec![],
        Err(CompileError::Parse(error)) => error.diagnostic_items(),
        Err(CompileError::Type(error)) => error.diagnostic_items(),
        Err(CompileError::Group(error)) => error.diagnostic_items(),
    }
}

// ── code actions / diagnostics ────────────────────────────────────

#[test]
fn map_function_typo_suggests_replacement() {
    // "rte" is close to "rate"
    let query = "ds:metric | map rte";
    let items = diagnostic_items(query);
    assert!(!items.is_empty(), "should produce a diagnostic");
    let item = &items[0];
    assert!(!item.actions.is_empty(), "should have code actions");
    assert_eq!(item.actions[0].insert, "rate");
}

#[test]
fn align_function_typo_suggests_replacement() {
    // "aveg" is close to "avg"
    let query = "ds:metric | align to 1m using aveg";
    let items = diagnostic_items(query);
    assert!(!items.is_empty());
    let item = &items[0];
    assert!(
        item.actions.iter().any(|a| a.insert == "avg"),
        "should suggest avg"
    );
}

#[test]
fn group_function_typo_suggests_replacement() {
    // "summ" is close to "sum"
    let query = "ds:metric | group using summ";
    let items = diagnostic_items(query);
    assert!(!items.is_empty());
    let item = &items[0];
    assert!(
        item.actions.iter().any(|a| a.insert == "sum"),
        "should suggest sum"
    );
}

#[test]
fn no_suggestion_for_unrelated_name() {
    // "zzzzz" has no similarity to any stdlib function
    let query = "ds:metric | map zzzzz";
    let items = diagnostic_items(query);
    assert!(!items.is_empty(), "should produce a diagnostic");
    let item = &items[0];
    assert!(
        item.actions.is_empty(),
        "should not suggest for unrelated names"
    );
}

#[test]
fn action_targets_function_name_range() {
    // The action's from/to should cover just the function name
    let query = "ds:metric | map rte";
    let items = diagnostic_items(query);
    let item = &items[0];
    let action = &item.actions[0];
    assert_eq!(&query[action.span.from..action.span.to], "rte");
}

#[test]
fn type_error_puts_error_on_use_and_info_on_declaration() {
    // $tag is declared as string but used where duration is expected
    let query = "param $tag: string;\nds:metric | align to $tag using avg";
    let items = match compile(query) {
        Ok(_) => panic!("should produce a type error"),
        Err(CompileError::Parse(_)) => panic!("should be a type error, not parse error"),
        Err(CompileError::Type(error)) => error.diagnostic_items(),
        Err(CompileError::Group(error)) => error.diagnostic_items(),
    };

    assert_eq!(items.len(), 2, "should produce two diagnostics");

    // The error should be on the usage site ($tag in align)
    let error_item = items.iter().find(|i| matches!(i.severity, Severity::Error));
    assert!(error_item.is_some(), "should have an error diagnostic");
    let error_item = error_item.unwrap();
    assert_eq!(
        &query[error_item.span.from..error_item.span.to],
        "$tag",
        "error should point at the usage of $tag"
    );

    // The info should be on the declaration site
    let info_item = items.iter().find(|i| matches!(i.severity, Severity::Info));
    assert!(info_item.is_some(), "should have an info diagnostic");
    let info_item = info_item.unwrap();
    assert!(
        info_item.message.contains("declaration"),
        "info message should mention declaration"
    );
}

#[test]
fn compute_function_typo_suggests_replacement() {
    // "minn" is close to "min"
    let query = "( ds1:m1 , ds2:m2 ) | compute result using minn";
    let items = diagnostic_items(query);
    assert!(!items.is_empty(), "should produce a diagnostic");
    let item = &items[0];
    assert!(
        item.actions.iter().any(|a| a.insert == "min"),
        "should suggest min, got actions: {:?}",
        item.actions.iter().map(|a| &a.insert).collect::<Vec<_>>()
    );
}

// ── dataset given, no metric ─────────────────────────────────────

fn assert_parse_error(query: &str, expected_from: usize, expected_to: usize) {
    let items = match compile(query) {
        Ok(_) => panic!("'{query}' should not compile"),
        Err(CompileError::Parse(error)) => error.diagnostic_items(),
        Err(CompileError::Type(_) | CompileError::Group(_)) => {
            panic!("'{query}' should be a parse error, not type/group error")
        }
    };
    assert_eq!(
        items.len(),
        1,
        "'{query}' should produce exactly one diagnostic"
    );
    assert!(
        matches!(items[0].severity, Severity::Error),
        "'{query}' should produce an error"
    );
    assert!(
        items[0].actions.is_empty(),
        "'{query}' should have no code actions"
    );
    assert_eq!(
        items[0].span.from, expected_from,
        "'{query}' error span.from"
    );
    assert_eq!(items[0].span.to, expected_to, "'{query}' error span.to");
}

#[test]
fn dataset_colon_no_metric_error_at_eof() {
    // "ds:" — error points at EOF (from=3, to=3)
    let query = "ds:";
    assert_parse_error(query, query.len(), query.len());
}

#[test]
fn backtick_dataset_colon_no_metric_error_at_eof() {
    // "`my-dataset`:" — error points at EOF
    let query = "`my-dataset`:";
    assert_parse_error(query, query.len(), query.len());
}

#[test]
fn dataset_no_colon_error_highlights_dataset() {
    // "ds" — error highlights "ds" as an unexpected token
    assert_parse_error("ds", 0, 2);
}

#[test]
fn dataset_no_metric_with_filter_error_at_pipe() {
    // "ds: | filter tag == \"x\"" — error highlights the "|"
    let query = "ds: | filter tag == \"x\"";
    assert_parse_error(query, 4, 5);
}

#[test]
fn dataset_no_colon_with_filter_error_highlights_dataset() {
    // "ds | filter tag == \"x\"" — error highlights "ds"
    assert_parse_error("ds | filter tag == \"x\"", 0, 2);
}

#[test]
fn backtick_dataset_no_metric_with_where_error_at_pipe() {
    // "`my-dataset`: | where tag == \"x\"" — error highlights the "|"
    let query = "`my-dataset`: | where tag == \"x\"";
    assert_parse_error(query, 14, 15);
}

#[test]
fn dataset_no_metric_with_time_range_error_at_bracket() {
    // "ds:[1h..]" — error highlights the "["
    assert_parse_error("ds:[1h..]", 3, 4);
}

// ── escaped ident dataset with dot, no colon ────────────────────

/// Runs compile → diagnostic_items → maybe_rewrite (the full wasm path).
fn diagnostics_for(query: &str) -> Vec<DiagnosticItem> {
    match compile(query) {
        Ok(_) => panic!("'{query}' should not compile"),
        Err(CompileError::Parse(error)) => {
            maybe_rewrite_escaped_dataset_error(query, error.diagnostic_items())
        }
        Err(CompileError::Type(_) | CompileError::Group(_)) => {
            panic!("'{query}' should be a parse error, not type error")
        }
    }
}

#[test]
fn backtick_dotted_dataset_no_colon_error_at_end_with_message() {
    let query = "`axiom-dev.metrics`";
    let items = diagnostics_for(query);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].severity, Severity::Error));
    assert_eq!(items[0].span.from, query.len(), "error should be at EOF");
    assert_eq!(items[0].span.to, query.len(), "error should be at EOF");
    assert!(
        items[0].message.contains("metric name"),
        "message should mention metric name, got: '{}'",
        items[0].message
    );
}

#[test]
fn backtick_dotted_dataset_suggests_colon_syntax() {
    let query = "`axiom-dev.metrics`";
    let items = diagnostics_for(query);
    assert!(
        items[0]
            .help
            .as_ref()
            .is_some_and(|h: &String| h.contains(':')),
        "help should mention ':' syntax, got: {:?}",
        items[0].help
    );
}

#[test]
fn backtick_dataset_no_dot_not_rewritten() {
    // No dot in the ident — rewrite should NOT fire, keep original behavior
    let query = "`my-dataset`";
    let items = diagnostics_for(query);
    assert_eq!(items.len(), 1);
    // Original error stays at position 0 (not rewritten)
    assert_eq!(items[0].span.from, 0);
}

#[test]
fn backtick_dataset_with_colon_not_rewritten() {
    // Has colon after ident — should NOT be rewritten
    let query = "`axiom-dev.metrics`:";
    let items = diagnostics_for(query);
    assert_eq!(items.len(), 1);
    // Original error, not our rewrite (error is at EOF for missing metric name)
    assert_ne!(
        items[0].message, "expected ':' and a metric name after the dataset",
        "should not rewrite when colon is present"
    );
}
