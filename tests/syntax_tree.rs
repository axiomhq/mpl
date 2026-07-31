use std::fs;

use miette::{GraphicalReportHandler, GraphicalTheme, NamedSource, Report};

#[test]
fn parse_examples() {
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());

    fs::read_dir("./tests/examples")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "mpl"))
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running example: {file_name}");
            let content = fs::read_to_string(&path).unwrap();
            let (tree, errors) = mpl_lang::syntax_tree::Parser::new(&content).parse();
            let n_errors = errors.len();
            for e in errors {
                let report =
                    Report::new(e).with_source_code(NamedSource::new(file_name, content.clone()));
                let mut error = String::new();
                if handler.render_report(&mut error, report.as_ref()).is_err() {
                    error = report.to_string();
                };

                eprintln!("{error}")
            }
            if n_errors != 0 {
                dbg!(tree);
            }
            assert_eq!(n_errors, 0);
        });
}

#[test]
fn parse_unimplemented_examples() {
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());

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
            let (tree, errors) = mpl_lang::syntax_tree::Parser::new(&content).parse();
            let n_errors = errors.len();
            for e in errors {
                let report =
                    Report::new(e).with_source_code(NamedSource::new(file_name, content.clone()));
                let mut error = String::new();
                if handler.render_report(&mut error, report.as_ref()).is_err() {
                    error = report.to_string();
                };

                eprintln!("{error}")
            }
            if n_errors != 0 {
                dbg!(tree);
            }
            assert_eq!(n_errors, 0);
        });
}

#[test]
fn parse_error_examples() {
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode());
    let failing = [
        "incomplete_query.mpl",
        "missing_pipe.mpl",
        "typo_keyword.mpl",
        "in-int.mpl",
        "invalid_time_unit.mpl",
        "in-trailing-comma.mpl",
        "invalid_operator.mpl",
    ];
    fs::read_dir("./tests/errors")
        .unwrap()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "mpl"))
        .for_each(|entry| {
            let path = entry.path();
            let file_name = path.file_name().unwrap().to_str().unwrap();
            println!("Running error case: {file_name}");
            let content = fs::read_to_string(&path).unwrap();
            let (tree, errors) = mpl_lang::syntax_tree::Parser::new(&content).parse();
            if !failing.contains(&file_name) {
                let n_errors = errors.len();
                for e in errors {
                    let report = Report::new(e)
                        .with_source_code(NamedSource::new(file_name, content.clone()));
                    let mut error = String::new();
                    if handler.render_report(&mut error, report.as_ref()).is_err() {
                        error = report.to_string();
                    };
                    eprintln!("{error}")
                }
                if n_errors != 0 {
                    for list in tree.children() {
                        let children = list
                            .children_with_tokens()
                            .map(|child| format!("{:?}@{:?}", child.kind(), child.text_range()))
                            .collect::<Vec<_>>();

                        dbg!(children);
                    }
                }
                assert_eq!(n_errors, 0);
            } else {
                assert!(!errors.is_empty());
            }
        });
}
