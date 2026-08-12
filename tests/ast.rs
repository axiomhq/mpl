//! Corpus tests for the AST lowering.
//!
//! The shape mirrors `tests/lex.rs`: run every shipped file through the layer and gate on
//! what it reported. What differs is the gate. The lexer answers per token, so `tests/lex.rs`
//! looks for the first `Token::is_invalid`; lowering answers per run, and it keeps going
//! after a failed rule so one pass collects as much as it can. That makes `Parser::errors` —
//! not `Parser::lower`'s return value — the thing to assert on.
//!
//! Every error counts, including the ones naming a production that has yet to be lowered.
//! The shipped examples are the definition of what this layer owes, so the list of files that
//! do not lower cleanly is the list of work left to do, and each test reports that list whole
//! rather than stopping at the first entry.
//!
//! Errors from the syntax tree arrive here too, wrapped as `ParserError::InvalidSyntax`, so a
//! file the parser rejects is rejected here as well. The error corpus therefore splits the
//! same way it does in `tests/syntax_tree.rs`, and the split is pinned by name: a file
//! drifting from one group to the other is a coverage change worth failing on.

use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};

use miette::{GraphicalReportHandler, GraphicalTheme, MietteDiagnostic, NamedSource, Report};
use mpl_lang::ast::{Parser, ParserError};
use test_case::test_case;

/// Every file under `dir` with the given extension, as `(file name, contents)`.
///
/// The emptiness check is the point: a renamed directory or extension would otherwise turn
/// every test built on this into a silent pass over nothing.
fn files(dir: &str, extension: &str) -> Vec<(String, String)> {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{dir} is not readable: {e}"))
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == extension))
        .map(|entry| {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let content = fs::read_to_string(&path).expect("readable example");
            (name, content)
        })
        .collect::<Vec<_>>();
    assert!(!entries.is_empty(), "no .{extension} files in {dir}");
    entries
}

/// The message carried by a caught panic.
fn panic_message(payload: &dyn std::any::Any) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panicked".to_string())
}

/// A message with no location attached.
fn bare(message: String) -> MietteDiagnostic {
    MietteDiagnostic {
        message,
        code: None,
        severity: None,
        help: None,
        url: None,
        labels: None,
    }
}

/// Folds a run's diagnostics into one, so a file with several errors renders as a single
/// snippet carrying every span rather than one snippet per error repeating the same source.
///
/// The messages are listed in `help` as well as pointed at by labels. The graphical handler
/// lays out one label per position, so the several zero-width labels an end-of-input error
/// produces all land on the same offset and share a slot; listing the messages is what keeps
/// the headline count honest about what the snippet can show. A lone diagnostic passes
/// through untouched, keeping its own code, help and severity.
fn merge(diagnostics: Vec<MietteDiagnostic>) -> Option<MietteDiagnostic> {
    let [only] = diagnostics.as_slice() else {
        let count = diagnostics.len();
        if count == 0 {
            return None;
        }
        let listed = diagnostics
            .iter()
            .enumerate()
            .map(|(i, diagnostic)| format!("{}. {}", i + 1, diagnostic.message))
            .collect::<Vec<_>>()
            .join("\n");
        let labels = diagnostics
            .into_iter()
            .filter_map(|diagnostic| diagnostic.labels)
            .flatten()
            .collect::<Vec<_>>();
        return Some(MietteDiagnostic {
            help: Some(listed),
            labels: (!labels.is_empty()).then_some(labels),
            ..bare(format!("{count} errors"))
        });
    };
    Some(only.clone())
}

/// Lowers `content`, rendering whatever went wrong as a single report over the file.
///
/// A production reached through `todo!()` unwinds, so the run is caught and the panic is
/// reported as the file's failure. That keeps one file's state from standing in for the rest
/// of the corpus, which is what a work list has to avoid.
fn lower(name: &str, content: &str) -> Option<String> {
    let diagnostics = catch_unwind(AssertUnwindSafe(|| {
        let parser = Parser::new(content);
        let ast = parser.lower();
        ast.errors
            .iter()
            .map(ParserError::to_diagnostic)
            .collect::<Vec<_>>()
    }))
    .unwrap_or_else(|payload| vec![bare(format!("panicked: {}", panic_message(&*payload)))]);

    let report = Report::new(merge(diagnostics)?)
        .with_source_code(NamedSource::new(name, content.to_string()));
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    let mut rendered = String::new();
    if handler
        .render_report(&mut rendered, report.as_ref())
        .is_err()
    {
        rendered = report.to_string();
    }
    Some(rendered)
}

/// Joins the per-file failures into one report, so a run names every file that needs work.
fn summarize(failures: &[(String, String)]) -> String {
    failures
        .iter()
        .map(|(name, rendered)| format!("[{name}]\n{rendered}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn lower_examples() {
    let corpus = files("./tests/examples", "mpl");
    let total = corpus.len();
    let failures = corpus
        .into_iter()
        .filter_map(|(name, content)| lower(&name, &content).map(|report| (name, report)))
        .collect::<Vec<_>>();

    assert!(
        failures.is_empty(),
        "{}\n{} of {total} examples do not lower cleanly",
        summarize(&failures),
        failures.len(),
    );
}

/// The `.unimplemented` examples are features the language does not have yet, kept next to
/// the working ones so the gap stays visible.
///
/// Each file's status is pinned rather than assumed, so when a feature lands the test fails
/// and the file gets moved to `tests/examples/*.mpl`.
#[test_case("enrich.mpl.unimplemented"         => false ; "enrich needs a join rule")]
#[test_case("nested-enrich.mpl.unimplemented"  => false ; "nested enrich needs a join rule")]
#[test_case("replace_labels.mpl.unimplemented" => false ; "replace labels needs query lowering")]
fn unimplemented_examples_lower(name: &str) -> bool {
    let content = fs::read_to_string(format!("./tests/examples/{name}"))
        .unwrap_or_else(|e| panic!("{name} is not readable: {e}"));
    lower(name, &content).is_none()
}

/// The error corpus is a mix: some files are rejected here, others lower cleanly and are
/// rejected by a later stage such as the type checker or the linker.
#[test]
fn lower_error_examples() {
    /// Files this layer rejects. Everything else in `tests/errors` is well-formed enough to
    /// lower and fails further down the pipeline.
    const REJECTED: &[&str] = &[
        "in-trailing-comma.mpl",
        "incomplete_query.mpl",
        "invalid_operator.mpl",
        "invalid_time_unit.mpl",
        "missing_pipe.mpl",
        "typo_keyword.mpl",
    ];

    let mut accepted_but_should_fail = Vec::new();
    let mut failures = Vec::new();

    for (name, content) in files("./tests/errors", "mpl") {
        match lower(&name, &content) {
            Some(report) if !REJECTED.contains(&name.as_str()) => failures.push((name, report)),
            None if REJECTED.contains(&name.as_str()) => accepted_but_should_fail.push(name),
            _ => {}
        }
    }

    assert!(
        accepted_but_should_fail.is_empty(),
        "expected a lowering error for: {}",
        accepted_but_should_fail.join(", ")
    );
    assert!(
        failures.is_empty(),
        "{}\n{} error-corpus files should reach a later stage but fail here.",
        summarize(&failures),
        failures.len(),
    );
}
