use std::collections::HashMap;

use mpl_lang::query::{WarningReason, Warnings};
use mpl_lang::{CompileError, compile};

use crate::diagnostics::{DiagnosticItem, Severity, maybe_rewrite_escaped_dataset_error};

fn diagnostic_items(q: &str) -> Vec<DiagnosticItem> {
    match compile(q, HashMap::new()) {
        Ok(_) => vec![],
        Err(CompileError::Parse(error)) => crate::diagnostics::parse_error_diagnostic_items(&error),
        Err(CompileError::Type(error)) => crate::diagnostics::type_error_diagnostic_items(&error),
        Err(CompileError::Group(error)) => crate::diagnostics::group_error_diagnostic_items(&error),
        Err(CompileError::Ifdef(error)) => crate::diagnostics::ifdef_error_diagnostic_items(&error),
    }
}

/// Run the full success-path pipeline: compile -> warnings -> diagnostic items.
fn warning_items(q: &str) -> Vec<DiagnosticItem> {
    let (_, warnings) = compile(q, HashMap::new()).expect("query should compile");
    warnings
        .as_slice()
        .iter()
        .map(crate::diagnostics::warning_to_diagnostic_item)
        .collect()
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
    let items = match compile(query, HashMap::new()) {
        Ok(_) => panic!("should produce a type error"),
        Err(CompileError::Parse(_)) => panic!("should be a type error, not parse error"),
        Err(CompileError::Type(error)) => crate::diagnostics::type_error_diagnostic_items(&error),
        Err(CompileError::Group(error)) => crate::diagnostics::group_error_diagnostic_items(&error),
        Err(CompileError::Ifdef(_)) => panic!("should be a type error, not ifdef error"),
    };

    assert_eq!(items.len(), 2, "should produce two diagnostics");

    // The error should be on the usage site ($tag in align)
    let error_item = items
        .iter()
        .find(|i| matches!(i.severity, Severity::Error))
        .expect("should have an error diagnostic");
    assert_eq!(
        &query[error_item.span.from..error_item.span.to],
        "$tag",
        "error should point at the usage of $tag"
    );

    // The info should be on the declaration site
    let info_item = items
        .iter()
        .find(|i| matches!(i.severity, Severity::Info))
        .expect("should have an info diagnostic");
    assert!(
        info_item.message.contains("declaration"),
        "info message should mention declaration"
    );
}

#[test]
fn optional_param_outside_ifdef_is_error() {
    let query = "param $f: Option<string>;\nds:metric | where tag == $f";
    let items = match compile(query, HashMap::new()) {
        Ok(_) => panic!("optional usage outside ifdef should not compile"),
        Err(CompileError::Ifdef(error)) => crate::diagnostics::ifdef_error_diagnostic_items(&error),
        Err(other) => panic!("expected ifdef error, got: {other}"),
    };

    assert_eq!(items.len(), 1, "should produce exactly one diagnostic");
    let item = &items[0];
    assert!(matches!(item.severity, Severity::Error));
    assert_eq!(
        &query[item.span.from..item.span.to],
        "$f",
        "error should point at the use site"
    );
    assert!(
        item.message.contains('f') && item.message.contains("ifdef"),
        "message should mention the param and ifdef, got: {:?}",
        item.message
    );
}

#[test]
fn ifdef_body_does_not_reference_param_is_error() {
    // The gating param `$f` is never referenced inside the ifdef body — that
    // means the ifdef is structurally pointless. The visitor catches this on
    // leave_ifdef.
    let query = "param $f: Option<string>;\nds:metric | ifdef($f) { where tag == \"x\" }";
    let items = match compile(query, HashMap::new()) {
        Ok(_) => panic!("ifdef body without param reference should not compile"),
        Err(CompileError::Ifdef(error)) => crate::diagnostics::ifdef_error_diagnostic_items(&error),
        Err(other) => panic!("expected ifdef error, got: {other}"),
    };

    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].severity, Severity::Error));
    assert!(
        items[0].message.contains("not referenced"),
        "message should describe the missing reference, got: {:?}",
        items[0].message
    );
}

#[test]
fn ifdef_body_referencing_param_compiles() {
    // Sanity: an ifdef whose body DOES reference the gating param compiles.
    let query = "param $f: Option<string>;\nds:metric | ifdef($f) { where tag == $f }";
    assert!(
        compile(query, HashMap::new()).is_ok(),
        "ifdef body referencing the gating param should compile"
    );
}

#[test]
fn optional_regex_param_outside_ifdef_is_error() {
    // Triggers OptionCheckVisitor::visit_parameterized_regex (the second emit
    // site of IfdefError::OptionalOutsideOfIfdef), distinct from the value path.
    let query = "param $r: Option<Regex>;\nds:metric | where tag == $r";
    let items = match compile(query, HashMap::new()) {
        Ok(_) => panic!("optional regex usage outside ifdef should not compile"),
        Err(CompileError::Ifdef(error)) => crate::diagnostics::ifdef_error_diagnostic_items(&error),
        Err(other) => panic!("expected ifdef error, got: {other}"),
    };

    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].severity, Severity::Error));
    assert_eq!(
        &query[items[0].span.from..items[0].span.to],
        "$r",
        "error should point at the regex use site"
    );
}

#[test]
fn optional_param_in_other_ifdef_is_error() {
    // $b is gated by ifdef($a), but referenced through ifdef($b)'s gate —
    // the visitor only allows the *same* optional param inside the ifdef.
    let query = concat!(
        "param $a: Option<string>;\n",
        "param $b: Option<string>;\n",
        "ds:metric | ifdef($a) { where tag == $b }",
    );
    let err = match compile(query, HashMap::new()) {
        Ok(_) => panic!("cross-ifdef optional should not compile"),
        Err(CompileError::Ifdef(error)) => error,
        Err(other) => panic!("expected ifdef error, got: {other}"),
    };
    let items = crate::diagnostics::ifdef_error_diagnostic_items(&err);
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].severity, Severity::Error));
    assert_eq!(
        &query[items[0].span.from..items[0].span.to],
        "$b",
        "error should point at the wrongly-gated param"
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

// ── parser-emitted warnings (OldDuration) ──────────────────────────

#[test]
fn old_duration_warning_emitted_as_diagnostic() {
    let query = "param $t: duration;\nds:metric | align to $t using avg";
    let items = warning_items(query);
    assert_eq!(items.len(), 1, "expected exactly one OldDuration warning");
    let item = &items[0];
    assert!(matches!(item.severity, Severity::Warning));
    assert!(
        item.message.contains("`duration`") && item.message.contains("Duration"),
        "warning message should mention both forms, got: {:?}",
        item.message
    );
    assert_eq!(
        &query[item.span.from..item.span.to],
        "duration",
        "warning span should cover the lowercase `duration` token"
    );
}

#[test]
fn old_duration_warning_has_replace_action() {
    let query = "param $t: duration;\nds:metric | align to $t using avg";
    let items = warning_items(query);
    let item = &items[0];
    assert_eq!(
        item.actions.len(),
        1,
        "OldDuration should have one quick-fix"
    );
    let action = &item.actions[0];
    assert_eq!(action.insert, "Duration");
    // The action's span must cover the exact `duration` token so applying it
    // is a straight substring replacement, not an offset shift.
    assert_eq!(&query[action.span.from..action.span.to], "duration");
    assert!(
        action.name.contains("Duration"),
        "action label should mention Duration, got: {:?}",
        action.name
    );
}

#[test]
fn uppercase_duration_emits_no_warning() {
    let query = "param $t: Duration;\nds:metric | align to $t using avg";
    let items = warning_items(query);
    assert!(items.is_empty(), "canonical `Duration` must not warn");
}

#[test]
fn param_not_declared_warning_is_plain_warning_without_actions() {
    // `ParamNotDeclared` is emitted from the runtime-param parsing path, not
    // from `compile`. We still translate it through the same conversion to
    // keep diagnostic surfaces uniform: severity=Warning, no quick-fix.
    let mut warnings = Warnings::new();
    warnings.push(WarningReason::ParamNotDeclared(vec!["$foo".to_string()]));
    let items: Vec<DiagnosticItem> = warnings
        .as_slice()
        .iter()
        .map(crate::diagnostics::warning_to_diagnostic_item)
        .collect();
    assert_eq!(items.len(), 1);
    assert!(matches!(items[0].severity, Severity::Warning));
    assert!(items[0].actions.is_empty());
    assert!(items[0].message.contains("$foo"));
}

#[test]
fn multiple_old_duration_warnings() {
    // Each occurrence of `duration` produces its own warning so the editor
    // can pin a quick-fix to every site, not just the first.
    let query = concat!(
        "param $t: duration;\n",
        "param $u: duration;\n",
        "ds:metric | align to $t using avg",
    );
    let items = warning_items(query);
    assert_eq!(items.len(), 2, "one warning per `duration` token");
    for item in &items {
        assert!(matches!(item.severity, Severity::Warning));
        assert_eq!(&query[item.span.from..item.span.to], "duration");
        assert_eq!(item.actions[0].insert, "Duration");
    }
}

// ── dataset given, no metric ─────────────────────────────────────

fn assert_parse_error(query: &str, expected_from: usize, expected_to: usize) {
    let items = match compile(query, HashMap::new()) {
        Ok(_) => panic!("'{query}' should not compile"),
        Err(CompileError::Parse(error)) => crate::diagnostics::parse_error_diagnostic_items(&error),
        Err(CompileError::Type(_) | CompileError::Group(_) | CompileError::Ifdef(_)) => {
            panic!("'{query}' should be a parse error, not type/group/ifdef error")
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

/// Runs `compile` → `diagnostic_items` → `maybe_rewrite` (the full wasm path).
fn diagnostics_for(query: &str) -> Vec<DiagnosticItem> {
    match compile(query, HashMap::new()) {
        Ok(_) => panic!("'{query}' should not compile"),
        Err(CompileError::Parse(error)) => maybe_rewrite_escaped_dataset_error(
            query,
            crate::diagnostics::parse_error_diagnostic_items(&error),
        ),
        Err(CompileError::Type(_) | CompileError::Group(_) | CompileError::Ifdef(_)) => {
            panic!("'{query}' should be a parse error, not type/group/ifdef error")
        }
    }
}

#[test]
fn backtick_dotted_dataset_no_colon_error_at_end_with_message() {
    let query = "`dev.metrics`";
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
    let query = "`dev.metrics`";
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
    let query = "`dev.metrics`:";
    let items = diagnostics_for(query);
    assert_eq!(items.len(), 1);
    // Original error, not our rewrite (error is at EOF for missing metric name)
    assert_ne!(
        items[0].message, "expected ':' and a metric name after the dataset",
        "should not rewrite when colon is present"
    );
}

// ── system_params plumbing ───────────────────────────────────────

/// Runs `compile` with caller-provided system params and converts the
/// resulting error (or empty success) to `DiagnosticItem`s, mirroring what
/// the wasm `diagnostics` entry point does after decoding the JS payload.
fn diagnostic_items_with_params(
    q: &str,
    params: HashMap<String, mpl_lang::query::ParamType>,
) -> Vec<DiagnosticItem> {
    match compile(q, params) {
        Ok(_) => vec![],
        Err(CompileError::Parse(error)) => crate::diagnostics::parse_error_diagnostic_items(&error),
        Err(CompileError::Type(error)) => crate::diagnostics::type_error_diagnostic_items(&error),
        Err(CompileError::Group(error)) => crate::diagnostics::group_error_diagnostic_items(&error),
        Err(CompileError::Ifdef(error)) => crate::diagnostics::ifdef_error_diagnostic_items(&error),
    }
}

#[test]
fn system_param_clears_undefined_param_error() {
    // Without system params, `$__interval` is undeclared and the parser
    // raises `UndefinedParam`. With it registered, the query compiles
    // cleanly — that's the whole point of the wiring.
    use mpl_lang::query::{ParamType, TerminalParamType};

    let query = "ds:metric | align to $__interval using avg";

    let without = diagnostic_items(query);
    assert!(
        !without.is_empty(),
        "without system params the reference should error"
    );

    let mut params = HashMap::new();
    params.insert(
        "__interval".to_string(),
        ParamType::Terminal(TerminalParamType::Duration),
    );
    let with = diagnostic_items_with_params(query, params);
    assert!(
        with.is_empty(),
        "with system params declared the query must compile, got {} items",
        with.len()
    );
}

#[test]
fn system_param_type_mismatch_still_errors() {
    // Registering a system param with the wrong type does NOT silence type
    // errors — `align to <duration>` rejects a string param, even when the
    // host claims `$__interval` is a string.
    use mpl_lang::query::{ParamType, TagType, TerminalParamType};

    let query = "ds:metric | align to $__interval using avg";
    let mut params = HashMap::new();
    params.insert(
        "__interval".to_string(),
        ParamType::Terminal(TerminalParamType::Tag(TagType::String)),
    );

    let items = diagnostic_items_with_params(query, params);
    assert!(
        !items.is_empty(),
        "type mismatch on a system param must still produce a diagnostic"
    );
    assert!(
        items.iter().any(|i| matches!(i.severity, Severity::Error)),
        "expected an error diagnostic, got messages: {:?}",
        items.iter().map(|i| &i.message).collect::<Vec<_>>()
    );
}

#[test]
fn system_param_missing_prefix_is_reported() {
    // System param names must start with `__` (SYSTEM_PARAM_PREFIX). The
    // parser surfaces this as a parse error; the editor relies on it to
    // tell hosts they've mis-registered a name.
    use mpl_lang::query::{ParamType, TerminalParamType};

    let query = "ds:metric";
    let mut params = HashMap::new();
    // No `__` prefix — invalid registration.
    params.insert(
        "interval".to_string(),
        ParamType::Terminal(TerminalParamType::Duration),
    );

    let items = diagnostic_items_with_params(query, params);
    assert!(
        items.iter().any(|i| i.message.contains("interval")),
        "missing-prefix error should mention the offending name, got messages: {:?}",
        items.iter().map(|i| &i.message).collect::<Vec<_>>()
    );
}
