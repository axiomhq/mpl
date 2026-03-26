//! MPL Parser

use std::{
    fs,
    io::{self, Read},
};

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
#[command(about = "MPL Parser")]
struct Args {
    /// Path to a .mpl file to parse (reads from stdin if not provided)
    file: Option<String>,

    /// Output format
    #[arg(short, long, value_enum, default_value = "debug")]
    format: Format,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let (content, source_name) = match args.file {
        Some(file_path) => {
            let content = fs::read_to_string(&file_path)
                .into_diagnostic()
                .map_err(|e| e.context(format!("Failed to read file '{file_path}'")))?;
            (content, file_path)
        }
        None => {
            let mut buffer = String::new();
            io::stdin()
                .read_to_string(&mut buffer)
                .into_diagnostic()
                .map_err(|e| e.context("Failed to read from stdin"))?;
            (buffer, "<stdin>".to_string())
        }
    };

    let parsed_query = mpl_lang::compile(&content).map_err(|e| {
        Report::new(e).with_source_code(NamedSource::new(&source_name, content.clone()))
    })?;

    let output = match args.format {
        Format::Json => serde_json::to_string_pretty(&parsed_query)
            .into_diagnostic()
            .map_err(|e| e.context("Failed to serialize to JSON"))?,
        Format::Ron => ron::ser::to_string_pretty(&parsed_query, ron::ser::PrettyConfig::default())
            .into_diagnostic()
            .map_err(|e| e.context("Failed to serialize to RON"))?,
        Format::Debug => format!("{parsed_query:?}"),
    };

    println!("{output}");
    Ok(())
}
