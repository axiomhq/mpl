//! Metrics Processing Language Command Line Interface
//!
//! The Metrics Processing Language Command Line Interface, MPL CLI, or
//! `mplc` is a command-line tool for working with mpl-lang, the Axion Metrics
//! Processing Language or MPL for short
#![deny(
    warnings,
    clippy::pedantic,
    clippy::unwrap_used,
    clippy::large_futures,
    missing_docs
)]

use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
    fs, io,
};

use clap::Parser;
use miette::{
    Diagnostic, IntoDiagnostic, LabeledSpan, MietteDiagnostic, NamedSource, Report, Result,
    SourceSpan,
};
use mpl_lang::{
    lexer::{Lexer, Token},
    query::{ParamType, TerminalParamType},
};

/// Output format
#[derive(Clone, Copy, clap::ValueEnum)]
enum Format {
    /// JSON output
    Json,
    /// RON (Rusty Object Notation) output
    Ron,
    /// Debug output
    Debug,
}

#[derive(Parser)]
#[command(name = "mplc")]
#[command(about = "MPL Command Line Interface")]
#[command(version)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

/// Corpus mode
#[derive(Default, Clone, Copy, clap::ValueEnum)]
enum CorpusMode {
    /// Full v1 style corpus parsing
    #[default]
    Full,
    /// v2 Lexer
    Lex,
    /// v2 lexer -> sytnax tree
    SyntaxTree,
    /// v2 lexer -> syntax tree -> ast
    Ast,
    /// Parse v2 the AST
    Parse,
}

impl CorpusMode {
    /// The name the mode is selected by on the command line, for the report header.
    fn name(self) -> &'static str {
        match self {
            CorpusMode::Full => "full",
            CorpusMode::Lex => "lex",
            CorpusMode::SyntaxTree => "syntax-tree",
            CorpusMode::Ast => "ast",
            CorpusMode::Parse => "parse",
        }
    }
}
#[derive(clap::Subcommand)]
enum Command {
    /// Parse an MPL file and output the AST
    Parse {
        /// Path to a .mpl file to parse
        file: String,

        /// Output format
        #[arg(short, long, value_enum, default_value = "ron")]
        format: Format,

        /// Write output to a file
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Parses a ndjson formated test corpus
    Corpus {
        /// the ndjson file to test
        file: String,
        /// whether to lex the corpus or fully parse it
        #[arg(short, long, value_enum, default_value = "full")]
        mode: CorpusMode,
        /// how many failing queries to render in full, all of them when unset
        #[arg(short, long)]
        limit: Option<usize>,
    },
}

/// One query lifted out of the corpus, with the context a report needs to point
/// back at where the query came from and how much it matters.
#[derive(Debug)]
struct CorpusEntry {
    /// 1-based line of the ndjson file the query was read from.
    line: usize,
    /// The query source.
    query: String,
    /// How often the query was seen; the corpus records this as `n`.
    occurrences: u64,
}

/// Reads one ndjson record, yielding the query it carries.
fn parse_entry(line: usize, text: &str) -> Result<Option<CorpusEntry>> {
    let record: serde_json::Value = serde_json::from_str(text)
        .into_diagnostic()
        .map_err(|e| e.context(format!("line {line} is not valid ndjson")))?;
    Ok(record
        .get("mpl")
        .and_then(serde_json::Value::as_str)
        .map(|query| CorpusEntry {
            line,
            query: query.to_string(),
            occurrences: record
                .get("n")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(1),
        }))
}

fn read_corpus(file: &str) -> Result<Vec<CorpusEntry>> {
    let content = fs::read_to_string(file)
        .into_diagnostic()
        .map_err(|e| e.context(format!("Failed to read file '{file}'")))?;
    content
        .lines()
        .enumerate()
        .map(|(idx, text)| parse_entry(idx + 1, text))
        .filter_map(Result::transpose)
        .collect()
}

/// The invalid tokens of a query gathered into one diagnostic, so a query with
/// several bad spans renders as a single snippet with a caret under each one.
fn invalid_tokens(query: &str) -> Option<MietteDiagnostic> {
    let labels: Vec<LabeledSpan> = Lexer::new(query)
        .filter(Token::is_invalid)
        .map(|token| {
            LabeledSpan::new_with_span(
                Some("invalid token".to_string()),
                SourceSpan::new(token.pos().into(), token.text().len()),
            )
        })
        .collect();
    let message = match labels.len() {
        0 => return None,
        1 => "invalid token".to_string(),
        n => format!("{n} invalid tokens"),
    };
    Some(
        MietteDiagnostic::new(message)
            .with_code("mplc::invalid_token")
            .with_labels(labels),
    )
}

/// Flattens an error into the parts a report renders.
fn flatten(error: &dyn Diagnostic) -> MietteDiagnostic {
    MietteDiagnostic {
        message: error.to_string(),
        code: error.code().map(|code| code.to_string()),
        severity: error.severity(),
        help: error.help().map(|help| help.to_string()),
        url: error.url().map(|url| url.to_string()),
        labels: error.labels().map(Iterator::collect),
    }
}

/// Expands `error` into the diagnostics a report should carry. An error that
/// wraps others says only that something went wrong; what it wraps says what,
/// and where, so the wrapped errors are what a row is counted and rendered
/// under. A wrapper that labels the source in its own right is kept alongside
/// them, and an error that wraps nothing stands for itself.
fn leaves(error: &dyn Diagnostic, out: &mut Vec<MietteDiagnostic>) {
    let diagnostic = flatten(error);
    let mut related = error.related().into_iter().flatten().peekable();
    let labels_the_source = diagnostic
        .labels
        .as_ref()
        .is_some_and(|labels| !labels.is_empty());
    if related.peek().is_none() || labels_the_source {
        out.push(diagnostic);
    }
    for inner in related {
        leaves(inner, out);
    }
}

/// Widens a label that covers nothing to the character it sits on, so it draws a
/// caret: an error at the end of input spans zero bytes and would otherwise render
/// as a bare source line with nothing pointing into it.
fn pointed(mut diagnostic: MietteDiagnostic, source_len: usize) -> MietteDiagnostic {
    let Some(last) = source_len.checked_sub(1) else {
        return diagnostic;
    };
    diagnostic.labels = diagnostic.labels.map(|labels| {
        labels
            .into_iter()
            .map(|label| {
                if !label.is_empty() {
                    return label;
                }
                let span = SourceSpan::new(label.offset().min(last).into(), 1);
                let text = label.label().map(ToString::to_string);
                if label.primary() {
                    LabeledSpan::new_primary_with_span(text, span)
                } else {
                    LabeledSpan::new_with_span(text, span)
                }
            })
            .collect()
    });
    diagnostic
}

/// Runs one query through `mode` and returns a diagnostic per problem found.
fn diagnose(
    entry: &CorpusEntry,
    mode: CorpusMode,
    system_params: &HashMap<String, ParamType>,
) -> Vec<MietteDiagnostic> {
    let diagnostics: Vec<MietteDiagnostic> = match mode {
        CorpusMode::Full => mpl_lang::compile(&entry.query, system_params.clone())
            .err()
            .iter()
            .flat_map(|error| {
                let mut out = Vec::new();
                leaves(error, &mut out);
                out
            })
            .collect(),
        CorpusMode::Parse => mpl_lang::compile2(&entry.query, system_params.clone())
            .err()
            .iter()
            .flat_map(|error| {
                let mut out = Vec::new();
                leaves(error, &mut out);
                out
            })
            .collect(),
        CorpusMode::Lex => invalid_tokens(&entry.query).into_iter().collect(),
        CorpusMode::SyntaxTree => mpl_lang::syntax_tree::Parser::new(&entry.query)
            .parse()
            .errors
            .iter()
            .flat_map(|error| {
                let mut out = Vec::new();
                leaves(error, &mut out);
                out
            })
            .collect(),
        // `to_diagnostic` flattens the syntax error a `ParserError` may wrap, which is
        // what carries the label pointing into the source.
        CorpusMode::Ast => mpl_lang::ast::Parser::new(&entry.query)
            .lower()
            .errors
            .iter()
            .map(mpl_lang::ast::AstError::to_diagnostic)
            .collect(),
    };
    diagnostics
        .into_iter()
        .map(|diagnostic| pointed(diagnostic, entry.query.len()))
        .collect()
}

/// Identifies a diagnostic across queries: its code and its message.
type Kind = (String, String);

/// The code a diagnostic is grouped under when it declares none.
const NO_CODE: &str = "-";

/// How much of the corpus one distinct diagnostic accounts for.
#[derive(Default)]
struct KindTally {
    /// Queries that produced the diagnostic.
    queries: usize,
    /// Corpus occurrences those queries stand for.
    occurrences: u64,
}

/// The kind a diagnostic is grouped under in the breakdown.
fn kind_of(diagnostic: &MietteDiagnostic) -> Kind {
    let code = diagnostic
        .code
        .clone()
        .unwrap_or_else(|| NO_CODE.to_string());
    (code, diagnostic.message.clone())
}

/// What a corpus run has found so far.
#[derive(Default)]
struct Findings {
    /// Queries that produced at least one diagnostic.
    failed: usize,
    /// Diagnostics found across those queries.
    diagnostics: usize,
    /// Failing queries rendered in full.
    rendered: usize,
    /// How much of the corpus each distinct diagnostic accounts for.
    kinds: HashMap<Kind, KindTally>,
}

impl Findings {
    /// Records one failing query, returning whether it is within the render limit.
    fn record(
        &mut self,
        entry: &CorpusEntry,
        found: &[MietteDiagnostic],
        limit: Option<usize>,
    ) -> bool {
        self.failed += 1;
        self.diagnostics += found.len();
        let render = limit.is_none_or(|limit| self.rendered < limit);
        if render {
            self.rendered += 1;
        }

        // A query hitting the same diagnostic twice is still one query, so it counts
        // its occurrences towards that kind once.
        let mut seen = HashSet::new();
        for kind in found.iter().map(kind_of) {
            if seen.insert(kind.clone()) {
                let tally = self.kinds.entry(kind).or_default();
                tally.queries += 1;
                tally.occurrences += entry.occurrences;
            }
        }
        render
    }
}

/// Renders the per-kind breakdown as a padded table, most frequent first.
///
/// The message sits on a continuation line under the code rather than in a column
/// of its own: messages quote source text, so a column wide enough for them wraps.
fn breakdown(kinds: &HashMap<Kind, KindTally>) -> String {
    const QUERIES: &str = "queries";
    const OCCURRENCES: &str = "occurrences";
    const DIAGNOSTIC: &str = "diagnostic";

    let mut rows: Vec<(&Kind, &KindTally)> = kinds.iter().collect();
    if rows.is_empty() {
        return String::new();
    }
    rows.sort_by(|((code_a, msg_a), a), ((code_b, msg_b), b)| {
        b.occurrences
            .cmp(&a.occurrences)
            .then(b.queries.cmp(&a.queries))
            .then_with(|| (code_a, msg_a).cmp(&(code_b, msg_b)))
    });

    let query_w = rows
        .iter()
        .map(|(_, tally)| tally.queries.to_string().len())
        .fold(QUERIES.len(), usize::max);
    let occurrence_w = rows
        .iter()
        .map(|(_, tally)| tally.occurrences.to_string().len())
        .fold(OCCURRENCES.len(), usize::max);
    let code_w = rows
        .iter()
        .map(|((code, _), _)| code.len())
        .fold(DIAGNOSTIC.len(), usize::max);

    let mut out = String::from("failures by kind:\n");
    let _ = writeln!(
        out,
        "  {QUERIES:>query_w$}  {OCCURRENCES:>occurrence_w$}  {DIAGNOSTIC}"
    );
    let _ = writeln!(
        out,
        "  {:->query_w$}  {:->occurrence_w$}  {:->code_w$}",
        "", "", ""
    );
    for ((code, message), tally) in rows {
        let (queries, occurrences) = (tally.queries, tally.occurrences);
        let _ = writeln!(
            out,
            "  {queries:>query_w$}  {occurrences:>occurrence_w$}  {code}"
        );
        let _ = writeln!(out, "  {:query_w$}  {:occurrence_w$}  {message}", "", "");
    }
    out
}

/// Checks every query in the corpus and writes a report: each failing query with
/// its diagnostics pointed at the offending source, then a breakdown and totals.
fn run_corpus(
    out: &mut impl io::Write,
    file: &str,
    mode: CorpusMode,
    limit: Option<usize>,
) -> Result<()> {
    let corpus = read_corpus(file)?;
    let system_params = system_params();

    let mut findings = Findings::default();

    for entry in &corpus {
        let found = diagnose(entry, mode, &system_params);
        if found.is_empty() {
            continue;
        }
        if findings.record(entry, &found, limit) {
            for diagnostic in found {
                // The source is named `<file>:<line>` so the snippet header reads
                // file, corpus line, then the line and column within the query.
                let report = Report::new(diagnostic).with_source_code(NamedSource::new(
                    format!("{file}:{}", entry.line),
                    entry.query.clone(),
                ));
                writeln!(out, "{report:?}\n").into_diagnostic()?;
            }
        }
    }

    let Findings {
        failed,
        diagnostics,
        rendered,
        kinds,
    } = findings;
    if failed > rendered {
        writeln!(
            out,
            "{} further failing queries not rendered, raise --limit to see them\n",
            failed - rendered
        )
        .into_diagnostic()?;
    }
    write!(out, "{}", breakdown(&kinds)).into_diagnostic()?;
    writeln!(
        out,
        "\n{file} ({}): total: {}, success: {}, failed: {failed}, errors: {diagnostics}",
        mode.name(),
        corpus.len(),
        corpus.len() - failed,
    )
    .into_diagnostic()
}

fn system_params() -> HashMap<String, ParamType> {
    let mut params = HashMap::new();
    params.insert(
        "__interval".to_string(),
        ParamType::Terminal(TerminalParamType::Duration),
    );
    params
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Corpus { file, mode, limit } => {
            run_corpus(&mut io::stdout().lock(), &file, mode, limit)?;
        }
        Command::Parse {
            file,
            format,
            output,
        } => {
            let content = fs::read_to_string(&file)
                .into_diagnostic()
                .map_err(|e| e.context(format!("Failed to read file '{file}'")))?;

            let system_params = system_params();

            let (parsed_query, _warnings) =
                mpl_lang::compile(&content, system_params).map_err(|e| {
                    Report::new(e).with_source_code(NamedSource::new(&file, content.clone()))
                })?;

            let output_str = match format {
                Format::Json => serde_json::to_string_pretty(&parsed_query)
                    .into_diagnostic()
                    .map_err(|e| e.context("Failed to serialize to JSON"))?,
                Format::Ron => {
                    ron::ser::to_string_pretty(&parsed_query, ron::ser::PrettyConfig::default())
                        .into_diagnostic()
                        .map_err(|e| e.context("Failed to serialize to RON"))?
                }
                Format::Debug => format!("{parsed_query:?}"),
            };

            if let Some(path) = output {
                fs::write(&path, &output_str)
                    .into_diagnostic()
                    .map_err(|e| e.context(format!("Failed to write to '{path}'")))?;
            } else {
                let lang = match format {
                    Format::Json => "json",
                    Format::Ron => "ron",
                    Format::Debug => {
                        println!("{output_str}");
                        return Ok(());
                    }
                };

                let theme = arborium::theme::builtin::catppuccin_mocha();
                let mut hl = arborium::AnsiHighlighter::new(theme);

                match hl.highlight(lang, &output_str) {
                    Ok(colored) => println!("{colored}"),
                    Err(_) => println!("{output_str}"),
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use miette::{GraphicalReportHandler, GraphicalTheme};

    /// Renders a diagnostic the way the corpus command does, with the theme pinned so
    /// an assertion does not depend on the terminal the test runs in.
    fn render(entry: &CorpusEntry, diagnostic: MietteDiagnostic) -> String {
        let report = Report::new(diagnostic).with_source_code(NamedSource::new(
            format!("corpus.ndjson:{}", entry.line),
            entry.query.clone(),
        ));
        let mut out = String::new();
        let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor());
        assert!(handler.render_report(&mut out, report.as_ref()).is_ok());
        out
    }

    fn entry(query: &str) -> CorpusEntry {
        CorpusEntry {
            line: 1,
            query: query.to_string(),
            occurrences: 1,
        }
    }

    /// The source text a rendered report draws its markers under.
    ///
    /// Columns are counted in characters: the gutter is drawn with box characters
    /// that are several bytes wide, so byte offsets do not line up between the source
    /// line and the marker line below it.
    fn pointed_at(rendered: &str) -> Option<String> {
        let source = rendered.lines().find(|line| line.contains('│'))?;
        let markers = rendered
            .lines()
            .find(|line| line.trim_start().starts_with('·'))?;
        let columns: Vec<usize> = markers
            .chars()
            .enumerate()
            .filter(|(_, c)| matches!(c, '▲' | '┬' | '─'))
            .map(|(column, _)| column)
            .collect();
        let (first, last) = (*columns.first()?, *columns.last()?);
        Some(source.chars().skip(first).take(last - first + 1).collect())
    }

    /// The line a query came from is what makes a diagnostic actionable: it is the
    /// only way back from a report to the record in a corpus of thousands.
    #[test]
    fn entry_carries_line_and_occurrences() {
        let record = r#"{"mpl": "a:b", "n": 754}"#;
        let entry = parse_entry(42, record)
            .expect("valid record")
            .expect("carries a query");
        assert_eq!(entry.line, 42);
        assert_eq!(entry.query, "a:b");
        assert_eq!(entry.occurrences, 754);
    }

    /// A corpus without counts still weighs each query, so the breakdown stays a
    /// query count rather than collapsing to zero.
    #[test]
    fn entry_without_count_stands_for_one_occurrence() {
        let entry = parse_entry(1, r#"{"mpl": "a:b"}"#)
            .expect("valid record")
            .expect("carries a query");
        assert_eq!(entry.occurrences, 1);
    }

    #[test]
    fn record_without_query_is_skipped() {
        let skipped = parse_entry(1, r#"{"n": 3}"#).expect("valid record");
        assert!(skipped.is_none());
    }

    #[test]
    fn malformed_record_names_its_line() {
        let error = parse_entry(7, "{not json").expect_err("invalid record");
        assert!(
            error.to_string().contains("line 7"),
            "expected the line in {error}"
        );
    }

    /// Every bad span of a query is labelled, not just the first one the lexer hits,
    /// so one render shows all the work a fix has to cover.
    #[test]
    fn invalid_tokens_label_every_bad_span() {
        let query = "a:b | filter x ~ 1 and y ~ 2";
        let diagnostic = invalid_tokens(query).expect("query has invalid tokens");
        let labels = diagnostic.labels.expect("labels point at the bad spans");
        let spanned: Vec<&str> = labels
            .iter()
            .filter_map(|label| query.get(label.offset()..label.offset() + label.len()))
            .collect();
        assert_eq!(spanned, ["~", "~"]);
        assert_eq!(diagnostic.message, "2 invalid tokens");
    }

    #[test]
    fn clean_query_lexes_without_a_diagnostic() {
        assert!(invalid_tokens("a:b | align to 1m using sum").is_none());
    }

    /// The point of the report: a failing query prints its own source with markers
    /// under the offending span, not a debug dump of a span offset.
    #[test]
    fn report_points_at_the_offending_source() {
        let entry = entry("a:b | filter x ~ 1");
        let mut found = diagnose(&entry, CorpusMode::Lex, &system_params());
        let rendered = render(&entry, found.pop().expect("query has an invalid token"));

        assert!(
            rendered.contains(&entry.query),
            "the source is missing from:\n{rendered}"
        );
        assert!(
            rendered.contains("corpus.ndjson:1"),
            "the corpus line is missing from:\n{rendered}"
        );
        assert_eq!(
            pointed_at(&rendered).as_deref(),
            Some("~"),
            "the markers miss the offending token in:\n{rendered}"
        );
    }

    /// A query that ends mid-command fails at a span covering no bytes at all, which
    /// renders as a bare source line; the report points at where the query ran out.
    #[test]
    fn report_points_at_the_end_of_a_truncated_query() {
        let entry = entry("a:b | filter");
        let found = diagnose(&entry, CorpusMode::SyntaxTree, &system_params());
        let eof = found
            .into_iter()
            .find(|diagnostic| diagnostic.message.contains("end of file"))
            .expect("a truncated query runs out of input");
        let rendered = render(&entry, eof);

        assert_eq!(
            pointed_at(&rendered).as_deref(),
            Some("r"),
            "nothing points at the end of the query in:\n{rendered}"
        );
    }

    /// The parse mode reports through an error that only wraps the ones the
    /// parser raised: its own sentence names no query and labels no source, so
    /// a report that counted it would say a query failed without saying what
    /// about it failed. What it wraps is what reaches the report.
    #[test]
    fn a_wrapping_error_reports_what_it_wraps() {
        let entry = entry("a:b | map nosuchfn()");
        let found = diagnose(&entry, CorpusMode::Parse, &system_params());

        assert!(
            !found.is_empty(),
            "the parse mode accepted an unknown function"
        );
        for diagnostic in &found {
            assert!(
                diagnostic
                    .labels
                    .as_ref()
                    .is_some_and(|labels| !labels.is_empty()),
                "{:?} points into no source",
                diagnostic.message
            );
        }
    }

    /// Every mode reports through the same path, so none of them can regress to a
    /// bare debug dump while the others render properly.
    #[test]
    fn every_mode_points_into_the_source() {
        let modes = [
            (CorpusMode::Full, "a:b | filter"),
            (CorpusMode::Parse, "a:b | map nosuchfn()"),
            (CorpusMode::Lex, "a:b | filter x == \"unterminated"),
            (CorpusMode::SyntaxTree, "a:b | filter"),
            (CorpusMode::Ast, "a:b | filter"),
        ];
        for (mode, query) in modes {
            let entry = entry(query);
            let mut found = diagnose(&entry, mode, &system_params());
            let diagnostic = found
                .pop()
                .unwrap_or_else(|| panic!("{} accepted {query}", mode.name()));
            let rendered = render(&entry, diagnostic);
            assert!(
                rendered.contains(query),
                "{} rendered no source:\n{rendered}",
                mode.name()
            );
            assert!(
                pointed_at(&rendered).is_some(),
                "{} rendered no markers:\n{rendered}",
                mode.name()
            );
        }
    }

    /// Corpora repeat the same query shapes, so the breakdown ranks by how much of
    /// the corpus a failure accounts for, not by how many distinct queries hit it.
    #[test]
    fn breakdown_ranks_by_occurrences_and_pads_columns() {
        let mut kinds = HashMap::new();
        kinds.insert(
            ("mpl_lang::rare".to_string(), "rare failure".to_string()),
            KindTally {
                queries: 9,
                occurrences: 12,
            },
        );
        kinds.insert(
            ("mpl_lang::common".to_string(), "common failure".to_string()),
            KindTally {
                queries: 2,
                occurrences: 3086,
            },
        );

        assert_eq!(
            breakdown(&kinds),
            "failures by kind:\n\
             \x20 queries  occurrences  diagnostic\n\
             \x20 -------  -----------  ----------------\n\
             \x20       2         3086  mpl_lang::common\n\
             \x20                       common failure\n\
             \x20       9           12  mpl_lang::rare\n\
             \x20                       rare failure\n"
        );
    }

    #[test]
    fn breakdown_of_a_clean_corpus_is_empty() {
        assert_eq!(breakdown(&HashMap::new()), "");
    }

    /// Line numbers count records of the file, not queries kept, so a line in the
    /// report still leads back to the record it was read from.
    #[test]
    fn corpus_lines_survive_skipped_records() {
        let path = std::env::temp_dir().join("mplc-read-corpus.ndjson");
        fs::write(
            &path,
            "{\"mpl\": \"a:b\"}\n{\"n\": 2}\n{\"mpl\": \"c:d\", \"n\": 7}\n",
        )
        .expect("temp corpus is writable");

        let corpus = read_corpus(&path.to_string_lossy()).expect("corpus reads back");

        let read: Vec<(usize, &str, u64)> = corpus
            .iter()
            .map(|entry| (entry.line, entry.query.as_str(), entry.occurrences))
            .collect();
        assert_eq!(read, [(1, "a:b", 1), (3, "c:d", 7)]);
    }

    fn diagnostic(code: &str, message: &str) -> MietteDiagnostic {
        MietteDiagnostic::new(message.to_string()).with_code(code.to_string())
    }

    /// A query that trips the same diagnostic twice is one query with one weight, or
    /// a query that fails late would outrank one that is simply more common.
    #[test]
    fn a_repeated_diagnostic_counts_its_query_once() {
        let entry = CorpusEntry {
            line: 1,
            query: "a:b".to_string(),
            occurrences: 5,
        };
        let found = [
            diagnostic("mpl_lang::eof", "unexpected end of file"),
            diagnostic("mpl_lang::eof", "unexpected end of file"),
        ];

        let mut findings = Findings::default();
        findings.record(&entry, &found, None);

        assert_eq!(findings.failed, 1);
        assert_eq!(findings.diagnostics, 2);
        let tally = findings
            .kinds
            .get(&(
                "mpl_lang::eof".to_string(),
                "unexpected end of file".to_string(),
            ))
            .expect("the kind was tallied");
        assert_eq!((tally.queries, tally.occurrences), (1, 5));
    }

    /// A diagnostic without a code still groups, under a placeholder, rather than
    /// dropping out of the breakdown.
    #[test]
    fn a_diagnostic_without_a_code_still_groups() {
        assert_eq!(
            kind_of(&MietteDiagnostic::new("bare".to_string())),
            (NO_CODE.to_string(), "bare".to_string())
        );
    }

    /// A label that already covers text is the parser's to place, so it is left as it
    /// is; only one covering nothing gets widened, and it keeps its role.
    #[test]
    fn only_empty_labels_are_widened() {
        let diagnostic = MietteDiagnostic::new("x".to_string()).with_labels([
            LabeledSpan::new_primary_with_span(None, SourceSpan::new(3.into(), 0)),
            LabeledSpan::new_with_span(None, SourceSpan::new(0.into(), 2)),
        ]);

        let labels = pointed(diagnostic, 4).labels.expect("the labels are kept");

        let spans: Vec<(usize, usize, bool)> = labels
            .iter()
            .map(|label| (label.offset(), label.len(), label.primary()))
            .collect();
        assert_eq!(spans, [(3, 1, true), (0, 2, false)]);
    }

    /// An empty query has no character to point at, so the label stays as it is
    /// rather than being widened past the end of the source.
    #[test]
    fn an_empty_query_keeps_its_label() {
        let diagnostic =
            MietteDiagnostic::new("x".to_string()).with_labels([LabeledSpan::new_with_span(
                None,
                SourceSpan::new(0.into(), 0),
            )]);

        let labels = pointed(diagnostic, 0).labels.expect("the labels are kept");

        assert_eq!(labels[0].len(), 0);
    }

    /// The mode names the report prints are the names the command line takes, so a
    /// mode renamed for clap cannot go on being reported under a stale name.
    #[test]
    fn mode_names_match_the_command_line() {
        use clap::ValueEnum as _;

        for mode in CorpusMode::value_variants() {
            let value = mode.to_possible_value().expect("every mode is selectable");
            assert_eq!(mode.name(), value.get_name());
        }
    }

    /// The report as a whole: a corpus on disk in, and for each failing record the
    /// source with markers under it, then the breakdown and the totals.
    #[test]
    fn a_corpus_file_reports_end_to_end() {
        let path = std::env::temp_dir().join("mplc-run-corpus.ndjson");
        fs::write(
            &path,
            "{\"mpl\": \"a:b | filter x ~ 1\", \"n\": 4}\n\
             {\"mpl\": \"a:b | align to 1m using sum\"}\n\
             {\"mpl\": \"c:d | filter y ~ 2\"}\n",
        )
        .expect("temp corpus is writable");
        let file = path.to_string_lossy().to_string();

        let mut out = Vec::new();
        run_corpus(&mut out, &file, CorpusMode::Lex, Some(1)).expect("the corpus reports");
        let report = String::from_utf8(out).expect("the report is text");

        assert!(
            report.contains("a:b | filter x ~ 1"),
            "the failing source is missing from:\n{report}"
        );
        assert_eq!(
            pointed_at(&report).as_deref(),
            Some("~"),
            "the markers miss the invalid token in:\n{report}"
        );
        assert!(
            report.contains("1 further failing queries not rendered"),
            "the capped query went unmentioned in:\n{report}"
        );
        assert!(
            report.contains("total: 3, success: 1, failed: 2, errors: 2"),
            "the totals are wrong in:\n{report}"
        );
        assert!(
            report.contains("        2            5  mplc::invalid_token"),
            "the breakdown is missing from:\n{report}"
        );
    }

    /// The limit caps rendering, not checking: every failing query is still counted
    /// so the totals describe the whole corpus.
    #[test]
    fn the_limit_caps_rendering_not_counting() {
        let found = [diagnostic("mpl_lang::eof", "unexpected end of file")];
        let mut findings = Findings::default();

        let rendered: Vec<bool> = (1..=3)
            .map(|line| {
                let entry = CorpusEntry {
                    line,
                    query: "a:b".to_string(),
                    occurrences: 1,
                };
                findings.record(&entry, &found, Some(2))
            })
            .collect();

        assert_eq!(rendered, [true, true, false]);
        assert_eq!(findings.failed, 3);
        assert_eq!(findings.rendered, 2);
    }
}
