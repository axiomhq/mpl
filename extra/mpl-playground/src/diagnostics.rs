//! Diagnostic formatting for playground-facing errors.

use std::fmt::Write as _;

use miette::Diagnostic as _;

pub(crate) fn message(source: &str, err: &mpl_lang::CompileError) -> String {
    let mut message = err.to_string();

    if let Some(labels) = err.labels() {
        for label in labels {
            let span = label.inner();
            let annotation = label.label().unwrap_or("error");
            let (line, column, text, line_start) = line_context(source, span.offset());
            let caret_start = span.offset().saturating_sub(line_start);
            let caret_len = span.len().max(1);
            let gutter = " ".repeat(line.to_string().len());
            let marker = if caret_len == 1 {
                "^---".to_string()
            } else {
                "^".repeat(caret_len)
            };

            let _ = write!(
                message,
                "\n --> {line}:{column}\n{gutter} |\n{line} | {text}\n{gutter} | {}{marker}\n{gutter} |\n{gutter} = {}",
                " ".repeat(caret_start),
                expected_annotation(annotation)
            );
        }
    }

    if let Some(help) = err.help() {
        message.push_str("\nhelp: ");
        message.push_str(&help.to_string());
    }

    message
}

fn expected_annotation(annotation: &str) -> String {
    let mut lines = annotation.lines();
    if lines.next() == Some("expected one of:") {
        let expected = lines
            .map(str::trim)
            .filter_map(|line| line.strip_prefix("- "))
            .map(|line| line.replace(' ', "_"))
            .collect::<Vec<_>>();

        return match expected.as_slice() {
            [single] => format!("expected {single}"),
            [] => annotation.to_string(),
            items => format!("expected one of {}", items.join(", ")),
        };
    }

    annotation.to_string()
}

fn line_context(source: &str, offset: usize) -> (usize, usize, &str, usize) {
    let bounded_offset = offset.min(source.len());
    let line_start = source[..bounded_offset]
        .rfind('\n')
        .map_or(0, |idx| idx + 1);
    let line_end = source[bounded_offset..]
        .find('\n')
        .map_or(source.len(), |idx| bounded_offset + idx);
    let line = source[..line_start].bytes().filter(|b| *b == b'\n').count() + 1;
    let column = source[line_start..bounded_offset].chars().count() + 1;

    (line, column, &source[line_start..line_end], line_start)
}

#[cfg(test)]
mod tests {
    use super::message;

    #[test]
    fn compile_errors_include_location_and_expected_tokens() {
        let source = "test:metric\n| map + ";
        let err = mpl_lang::compile(source).expect_err("query should fail to compile");
        let message = message(source, &err);

        assert!(message.contains("MPL syntax error"), "{message}");
        assert!(message.contains("--> 2:"), "{message}");
        assert!(message.contains("| map +"), "{message}");
        assert!(message.contains("^---"), "{message}");
        assert!(message.contains("= expected"), "{message}");
    }
}
