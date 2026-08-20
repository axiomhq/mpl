use std::collections::HashMap;

use mpl_lang::query::{WarningReason, Warnings};
use mpl_lang::{CompileError, compile};

use crate::diagnostics::{DiagnosticItem, Severity, compute_diagnostics};

/// The diagnostics the editor is handed for `q`.
///
/// Every helper here goes through `compute_diagnostics`, so a case states what
/// a user sees rather than what one conversion produces in isolation.
fn diagnostic_items(q: &str) -> Vec<DiagnosticItem> {
    compute_diagnostics(q, &HashMap::new())
}

/// The error diagnostics for `q`, dropping warnings and hints.
fn error_items(q: &str) -> Vec<DiagnosticItem> {
    diagnostic_items(q)
        .into_iter()
        .filter(|i| matches!(i.severity, Severity::Error))
        .collect()
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
    // $d is declared as a duration but compared against a tag
    let query = "param $d: Duration;\nds:metric | filter tag == $d";
    let items = match compile(query, HashMap::new()) {
        Ok(_) => panic!("should produce a type error"),
        Err(CompileError::Type(error)) => crate::diagnostics::type_error_diagnostic_items(&error),
        Err(CompileError::Group(error)) => crate::diagnostics::group_error_diagnostic_items(&error),
        Err(CompileError::Ifdef(_)) => panic!("should be a type error, not ifdef error"),
        Err(CompileError::Parser(_)) => panic!("should be a type error, not a parser error"),
    };

    assert_eq!(items.len(), 2, "should produce two diagnostics");

    // The error should be on the usage site ($d in the filter)
    let error_item = items
        .iter()
        .find(|i| matches!(i.severity, Severity::Error))
        .expect("should have an error diagnostic");
    assert_eq!(
        &query[error_item.span.from..error_item.span.to],
        "$d",
        "error should point at the usage of $d"
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

// ── parser-emitted warnings ───────────────────────────────────────

#[test]
fn uppercase_duration_emits_no_warning() {
    let query = "param $t: Duration;\nds:metric | align to $t using avg";
    let items = warning_items(query);
    assert!(items.is_empty(), "canonical `Duration` must not warn");
}

#[test]
fn in_filter_with_array_literal_is_clean() {
    // `in` with an array literal is valid MPL end-to-end: no parse errors and
    // no warnings surface as diagnostics.
    let query = "ds:metric | where code in [200, 201, \"a\"]";
    assert!(diagnostic_items(query).is_empty(), "should not error");
    assert!(warning_items(query).is_empty(), "should not warn");
}

#[test]
fn in_filter_with_scalar_rhs_errors() {
    // The parser only accepts an array literal on the RHS of `in`; a scalar
    // must surface as an error diagnostic rather than compiling.
    let query = "ds:metric | where code in 200";
    let items = diagnostic_items(query);
    assert!(!items.is_empty(), "scalar RHS for `in` should diagnose");
    assert!(matches!(items[0].severity, Severity::Error));
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

// ── dataset given, no metric ─────────────────────────────────────

/// Asserts the editor is told about `query` at `expected_from..expected_to`.
///
/// The parser recovers and reports what each stage tripped over, so a case names the span it
/// cares about rather than how many diagnostics reach the editor alongside it.
fn assert_parse_error(query: &str, expected_from: usize, expected_to: usize) {
    let items = error_items(query);
    assert!(!items.is_empty(), "'{query}' should not compile");
    assert!(
        items
            .iter()
            .any(|i| i.span.from == expected_from && i.span.to == expected_to),
        "'{query}' should report at {expected_from}..{expected_to}, got: {:?}",
        items
            .iter()
            .map(|i| (i.span.from, i.span.to, &i.message))
            .collect::<Vec<_>>()
    );
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
/// A source names a dataset and a metric, so a lone name is reported where the `:` that
/// would separate them belongs.
fn dataset_no_colon_error_points_at_the_missing_colon() {
    assert_parse_error("ds", 2, 2);
}

#[test]
fn dataset_no_metric_with_filter_error_at_pipe() {
    // "ds: | filter tag == \"x\"" — error highlights the "|"
    let query = "ds: | filter tag == \"x\"";
    assert_parse_error(query, 4, 5);
}

#[test]
/// The `|` is the first token that cannot follow a bare dataset name, so it is what the
/// missing `:` is reported against.
fn dataset_no_colon_with_filter_error_points_at_the_pipe() {
    assert_parse_error("ds | filter tag == \"x\"", 3, 4);
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
/// The message the escaped-dataset rewrite puts in place of the parser's own.
const REWRITTEN_MESSAGE: &str = "expected ':' and a metric name after the dataset";

fn diagnostics_for(query: &str) -> Vec<DiagnosticItem> {
    let items = error_items(query);
    assert!(!items.is_empty(), "'{query}' should not compile");
    items
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

/// The rewrite reads a dataset and a metric out of one escaped identifier, so it
/// only has something to say when there is a dot to split on.
#[test]
fn backtick_dataset_no_dot_not_rewritten() {
    let query = "`my-dataset`";
    let items = diagnostics_for(query);
    assert!(
        !items.iter().any(|i| i.message == REWRITTEN_MESSAGE),
        "'{query}' names no metric to split out, got: {:?}",
        items.iter().map(|i| &i.message).collect::<Vec<_>>()
    );
}

#[test]
fn backtick_dataset_with_colon_not_rewritten() {
    // Has colon after ident — should NOT be rewritten
    let query = "`dev.metrics`:";
    let items = diagnostics_for(query);
    assert!(
        !items.iter().any(|i| i.message == REWRITTEN_MESSAGE),
        "'{query}' already separates dataset and metric, got: {:?}",
        items.iter().map(|i| &i.message).collect::<Vec<_>>()
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
    compute_diagnostics(q, &params)
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

// ── parser errors and warnings ───────────────────────────────────

/// A parser error reaches the editor anchored to the token it is about.
/// Driven through `compile` rather than a hand-built error, so the offsets
/// asserted here are the ones a user would see highlighted.
#[test]
fn a_parser_error_is_anchored_to_its_token() {
    let query = "d:m | map unknownfn()";
    let Err(CompileError::Parser(errors)) = compile(query, HashMap::new()) else {
        panic!("{query:?} should fail in the parser")
    };

    let items = crate::diagnostics::parser_error_diagnostic_items(query, &errors);

    assert_eq!(items.len(), 1, "one error should yield one diagnostic");
    assert!(matches!(items[0].severity, Severity::Error));
    assert_eq!(
        (items[0].span.from, items[0].span.to),
        (10, 21),
        "the diagnostic should cover `unknownfn()`"
    );
    assert!(
        items[0].message.contains("unknownfn"),
        "the message should name the function: {:?}",
        items[0].message
    );
}

/// Every error reported produces at least one diagnostic, and one that
/// labels nothing is anchored at the start of the query. Stated as a count
/// relation rather than an exact list, because what matters is that a
/// failure is never swallowed on the way to the editor.
#[test]
fn no_parser_error_is_dropped() {
    let Err(CompileError::Parser(errors)) = compile("", HashMap::new()) else {
        panic!("an empty query should fail in the parser")
    };

    let items = crate::diagnostics::parser_error_diagnostic_items("", &errors);

    assert!(
        items.len() >= errors.len(),
        "{} errors collapsed into {} diagnostics",
        errors.len(),
        items.len()
    );
    assert!(
        items
            .iter()
            .any(|i| i.message.contains("No query") && (i.span.from, i.span.to) == (0, 0)),
        "the unlabelled error should still be surfaced, at the start of the query"
    );
}

/// A warning the parser raised is surfaced as a warning, at the span it
/// was raised for — `"x\qy"` flags the unknown escape at the string.
#[test]
fn a_parser_warning_keeps_its_span() {
    let query = r#"d:m | where a == "x\qy""#;
    let (_, warnings) = compile(query, HashMap::new()).expect("the query should compile");

    let items: Vec<_> = warnings
        .as_slice()
        .iter()
        .map(crate::diagnostics::warning_to_diagnostic_item)
        .collect();

    assert_eq!(items.len(), 1, "the unknown escape should warn once");
    assert!(matches!(items[0].severity, Severity::Warning));
    assert_eq!(
        (items[0].span.from, items[0].span.to),
        (17, 23),
        "the warning should cover the string literal"
    );
}

/// `compute_diagnostics` is the entry point the editor calls, so these cases read what an
/// editor would draw rather than what one stage of the compiler produced.
mod compute {
    use std::collections::HashMap;

    use crate::diagnostics::{Severity, compute_diagnostics};

    fn errors(query: &str) -> Vec<(usize, usize, String)> {
        compute_diagnostics(query, &HashMap::new())
            .into_iter()
            .filter(|d| matches!(d.severity, Severity::Error))
            .map(|d| (d.span.from, d.span.to, d.message))
            .collect()
    }

    /// A squiggle is only useful where the problem is. An error that names no place in the
    /// query would be drawn at the very start of the document, pointing the reader away from
    /// the token that caused it.
    #[test]
    fn a_syntax_error_is_anchored_where_it_happened() {
        let query = "test:metric\n| map + ";
        let found = errors(query);
        assert!(!found.is_empty(), "expected an error for `{query}`");
        assert!(
            found.iter().all(|(from, to, _)| *from > 0 && *to > 0),
            "every error should point past the start of the query, got: {found:?}"
        );
    }

    /// The parser reports what it recovered from at several stages, so the same problem can
    /// arrive more than once; an editor draws one squiggle per diagnostic.
    #[test]
    fn the_same_problem_is_reported_once() {
        let found = errors("ds:metric | where");
        let mut seen = found.clone();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), found.len(), "duplicate diagnostics: {found:?}");
    }

    /// `in` compares against a set, so a scalar right-hand side is a query the editor has to
    /// reject rather than pass to the backend.
    #[test]
    fn in_with_a_scalar_is_reported() {
        let found = errors("ds:metric | where t in 200");
        assert!(
            found.iter().any(|(.., m)| m.contains("requires an array")),
            "expected an in-requires-array diagnostic, got: {found:?}"
        );
    }

    #[test]
    fn a_valid_query_reports_nothing() {
        assert_eq!(errors("ds:metric | where t == 200"), Vec::new());
    }
}
