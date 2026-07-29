//! Metrics Processing Language Command Line Interface
//!
//! The Metrics Processing Language Command Line Interface, MPL CLI, or
//! `mplc` is a command-line tool for working with mpl-lang, the Axion Metrics
//! Processing Language or MPL for short

use std::{collections::HashMap, fs};

use clap::Parser;
use miette::{IntoDiagnostic, NamedSource, Report, Result};
use mpl_lang::{
    lexer::Token,
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
        #[arg(short, long)]
        lex: bool,
    },
}

fn read_corpus(file: &str) -> Result<Vec<String>> {
    let content = fs::read_to_string(&file)
        .into_diagnostic()
        .map_err(|e| e.context(format!("Failed to read file '{file}'")))?;
    content
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l))
        .filter_map(|l| {
            l.map(|l| -> Option<String> {
                l.get("mpl").and_then(|v| v.as_str()).map(|s| s.to_string())
            })
            .transpose()
        })
        .collect::<Result<Vec<String>, serde_json::Error>>()
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
        Command::Corpus { file, lex } => {
            let corpus = read_corpus(&file)?;

            let mut success = 0;
            let error_cnt;
            if lex {
                let mut errors = Vec::new();
                for c in &corpus {
                    let mut lexed = mpl_lang::lexer::Lexer::new(c.as_str());

                    if let Some(t) = lexed.find(Token::is_invalid) {
                        errors.push((c.clone(), t));
                    } else {
                        success += 1;
                    }
                }
                error_cnt = errors.len();
                for (q, t) in &errors {
                    println!("error: {q}: {t:?}");
                }
            } else {
                let system_params = system_params();
                let mut errors = Vec::new();

                for c in &corpus {
                    let params = system_params.clone();
                    let parsed = mpl_lang::compile(c.as_str(), params).map_err(|e| {
                        Report::new(e).with_source_code(NamedSource::new(&file, c.clone()))
                    });
                    match parsed {
                        Ok(_) => success += 1,
                        Err(e) => errors.push((c.clone(), e)),
                    }
                }
                error_cnt = errors.len();
                for (q, e) in &errors {
                    println!("error: {q}: {e}");
                }
            }
            println!(
                "total: {}, success: {success}, errors: {error_cnt}",
                corpus.len(),
            );
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

            match output {
                Some(path) => {
                    fs::write(&path, &output_str)
                        .into_diagnostic()
                        .map_err(|e| e.context(format!("Failed to write to '{path}'")))?;
                }
                None => {
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
    }

    Ok(())
}
