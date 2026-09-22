// SPDX-FileCopyrightText: 2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

//! etradeAnonymizer - PDF anonymization tool for E*TRADE / Morgan Stanley statements.
//!
//! This tool provides two subcommands:
//! - `list`: List all text tokens from FlateDecode streams in a PDF
//! - `anonymize`: Anonymize PDF by replacing all PII except calculation-relevant data
//!
//! The tool operates on tightly structured PDF FlateDecode streams and preserves
//! the original file structure by performing in-place replacements with exact-size matching.

// Submodules are exported by `src/anonymizer/mod.rs` via the library crate.
// Since this file is a binary entry point, reference them via `etradeTaxReturnHelper::anonymizer::...`.

use clap::{Parser, Subcommand};
use etradeTaxReturnHelper::anonymizer;
use std::error::Error;
use std::path::PathBuf;

/// Tool for anonymizing PDF files by replacing specific strings in FlateDecode streams
#[derive(Parser)]
#[command(name = "etradeAnonymizer")]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List all text tokens from FlateDecode streams in the PDF
    List {
        /// Path to the input PDF file
        input_file: PathBuf,
    },
    /// Anonymize PDF by replacing all text except calculation-relevant data
    Anonymize {
        /// Path to the input PDF file
        input_file: PathBuf,
        /// Path to the output PDF file (optional, defaults to anonymous_<input>.pdf)
        output_file: Option<PathBuf>,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    // Default to `warn` level; RUST_LOG env var overrides this if set.
    simple_logger::SimpleLogger::new()
        .with_level(log::LevelFilter::Warn)
        .env()
        .init()
        .unwrap();

    let cli = Cli::parse();

    match cli.command {
        Commands::List { input_file } => anonymizer::list::list_texts(&input_file),
        Commands::Anonymize {
            input_file,
            output_file,
        } => {
            let output =
                output_file.unwrap_or_else(|| anonymizer::path::anonymous_output_path(&input_file));
            anonymizer::replace::replace_pii_smart(&input_file, &output)
        }
    }
}
