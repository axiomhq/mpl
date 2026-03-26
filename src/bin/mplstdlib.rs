//! MPL Parser

use clap::Parser;
use mpl_lang::STDLIB;

/// Output format
#[derive(Clone, Copy, clap::ValueEnum)]
enum Format {
    /// JSON output
    Json,
    /// Debug output
    Markdown,
}

#[derive(Parser)]
#[command(name = "mplstdlib")]
#[command(about = "MPL Standard Library")]
struct Args {
    /// Output format
    #[arg(short, long, value_enum, default_value = "markdown")]
    format: Format,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match args.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(&*STDLIB)?),
        Format::Markdown => println!("{}", STDLIB.documentation(0)?),
    }

    Ok(())
}
