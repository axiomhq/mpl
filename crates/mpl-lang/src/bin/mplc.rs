//! Metrics Processing Language Command Line Interface
//!
//! The Metrics Processing Language Command Line Interface, MPL CLI, or
//! `mplc` is a command-line tool for working with mpl-lang, the Axion Metrics
//! Processing Language or MPL for short

use std::fs;

use clap::Parser;
use miette::{IntoDiagnostic, NamedSource, Report, Result};

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
}

fn main() -> Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Parse {
            file,
            format,
            output,
        } => {
            let content = fs::read_to_string(&file)
                .into_diagnostic()
                .map_err(|e| e.context(format!("Failed to read file '{file}'")))?;

            let parsed_query = mpl_lang::compile(&content).map_err(|e| {
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
