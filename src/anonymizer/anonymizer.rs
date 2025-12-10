// SPDX-FileCopyrightText: 2024-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

mod list;
mod detect;
mod path;
mod pdf;
mod replace;

use clap::{Parser, Subcommand};
use std::env;
use std::error::Error;

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
        input_file: String,
    },
    /// Detect PII (name, address, account) in the PDF and print replacement command
    Detect {
        /// Path to the input PDF file
        input_file: String,
    },
    /// Replace strings in PDF FlateDecode streams and save to output file
    Replace {
        /// Path to the input PDF file
        input_file: String,
        /// Path to the output PDF file
        output_file: String,
        /// Pairs of strings to replace: <search> <replacement> <search> <replacement> ...
        #[arg(required = true, num_args = 2..)]
        replacements: Vec<String>,
    },
}

fn main() -> Result<(), Box<dyn Error>> {
    // Ensure users see warnings and errors by default even when RUST_LOG is not set.
    // If RUST_LOG is provided, simple_logger will respect it; otherwise we default to `warn`.
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "warn");
    }
    simple_logger::SimpleLogger::new().env().init().unwrap();

    let cli = Cli::parse();

    match cli.command {
        Commands::List { input_file } => list::list_texts(&input_file),
        Commands::Detect { input_file } => detect::detect_pii(&input_file),
        Commands::Replace {
            input_file,
            output_file,
            replacements,
        } => {
            if replacements.len() % 2 != 0 {
                return Err("Replacements must be provided as pairs: <search> <replacement>".into());
            }
            let mut replacement_pairs: Vec<(String, String)> = Vec::new();
            let mut i = 0;
            while i < replacements.len() {
                replacement_pairs.push((replacements[i].clone(), replacements[i + 1].clone()));
                i += 2;
            }
            replace::replace_mode(&input_file, &output_file, replacement_pairs)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Note: These tests require 'anonymizer_data' directory to be present in the working directory
    // when running 'cargo test'.

    #[test]
    fn test_detect_mode() -> Result<(), Box<dyn Error>> {
        let sample = "anonymizer_data/sample_statement.pdf";
        assert!(std::path::Path::new(sample).exists(), "Required test file missing: {}", sample);

        detect::detect_pii(sample)?;
        Ok(())
    }

    #[test]
    fn test_replace_mode() -> Result<(), Box<dyn Error>> {
        let sample = "anonymizer_data/sample_statement.pdf";
        let expected_pdf = "anonymizer_data/expected_statement.pdf";
        let output_dir = "target/test_outputs";
        let output_pdf = "target/test_outputs/out_sample_statement.pdf";

        assert!(std::path::Path::new(sample).exists(), "Required test file missing: {}", sample);
        assert!(std::path::Path::new(expected_pdf).exists(), "Required test file missing: {}", expected_pdf);

        fs::create_dir_all(output_dir)?;

        let replacements = vec![
            ("JAN KOWALSKI".to_string(), "XXXXXXXXXXXX".to_string()),
            ("UL. SWIETOKRZYSKA 12".to_string(), "XXXXXXXXXXXXXXXXXXXX".to_string()),
            ("WARSAW 00-916 POLAND".to_string(), "XXXXXXXXXXXXXXXXXXXX".to_string()),
            ("012 - 345678 - 910 -".to_string(), "XXXXXXXXXXXXXXXXXXXX".to_string()),
            ("012-345678-910".to_string(), "XXXXXXXXXXXXXX".to_string()),
        ];

        replace::replace_mode(sample, output_pdf, replacements)?;

        let produced = fs::read(output_pdf)?;
        let expected = fs::read(expected_pdf)?;
        assert_eq!(produced, expected, "produced PDF differs from expected");

        // Cleanup
        let _ = fs::remove_file(output_pdf);
        Ok(())
    }
}
