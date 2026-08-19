use std::collections::HashMap;
use std::fs;

use miette::{GraphicalReportHandler, GraphicalTheme, NamedSource, Report};
use mpl_lang::query::{ParamType, TerminalParamType};
use mpl_lang::{CompileError, ParseError};

/// Renders `err` the way a user would see it: the labels point into the
/// example and the report is named for the file it came from, so a failure
/// says where in the query it happened rather than only what went wrong.
/// An error carrying related reports renders all of them.
///
/// The theme is pinned to the colour one, so the rendering is the same
/// whichever terminal the test runs in.
fn report(file_name: &str, content: &str, err: CompileError) -> String {
    let report =
        Report::new(err).with_source_code(NamedSource::new(file_name, content.to_string()));
    let mut out = String::new();
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    if handler.render_report(&mut out, report.as_ref()).is_err() {
        out = report.to_string();
    }
    out
}

/// Asserts one entry point compiled `content`, reporting a rejection as the
/// rendered diagnostic. `label` names the entry point, so an example that
/// only one of them rejects says which one.
fn check<T>(label: &str, file_name: &str, content: &str, r: Result<T, CompileError>) {
    match r {
        Ok(_) => println!("  {label}: parsed successfully"),
        Err(CompileError::Parse(ParseError::NotImplemented(feature))) => {
            println!("  {label}: parsed, not yet implemented: {feature}");
        }
        Err(e) => panic!(
            "{label} rejected {file_name}:\n{}",
            report(file_name, content, e)
        ),
    }
}

#[test]
fn parse_examples() {
    fs::read_dir("./tests/examples")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "mpl"))
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running example: {file_name}");
            let content = fs::read_to_string(&path).unwrap();
            let mut params = HashMap::new();
            params.insert(
                "__interval".to_string(),
                ParamType::Terminal(TerminalParamType::Duration),
            );
            check(
                "compile",
                file_name,
                &content,
                mpl_lang::compile(&content, params.clone()),
            );
            check(
                "compile",
                file_name,
                &content,
                mpl_lang::compile(&content, params),
            );
        });
}

#[test]
fn parse_unimplemented_examples() {
    fs::read_dir("./tests/examples")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == "mpl-todo")
        })
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running example: {file_name}");
            let content = fs::read_to_string(&path).unwrap();
            match mpl_lang::compile(&content, HashMap::new()) {
                Ok(_) => panic!("{file_name} compiled but is expected to fail"),
                Err(e) => println!("Failing as expected:\n{}", report(file_name, &content, e)),
            }
        });
}

#[test]
fn parse_error_examples() {
    fs::read_dir("./tests/errors")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "mpl"))
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running error case: {file_name}");
            let content = fs::read_to_string(&path).unwrap();
            match mpl_lang::compile(&content, HashMap::new()) {
                Ok(_) => panic!("{file_name} compiled but is expected to fail"),
                Err(e) => println!("Failing as expected:\n{}", report(file_name, &content, e)),
            }
        });
}
