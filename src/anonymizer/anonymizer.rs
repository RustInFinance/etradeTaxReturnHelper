// SPDX-FileCopyrightText: 2024-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

mod detect;
mod pdf;
mod replace;

use std::env;

/// Entry point for programmatic invocation and CLI help text.
fn help_text() -> &'static str {
    "etradeAnonymizer - Tool for anonymizing PDF files by replacing specific strings in FlateDecode streams.\n\
	\nUsage:\n\
	  etradeAnonymizer detect <input_file_path>\n\
	  etradeAnonymizer replace <input_file_path> <output_file_path> <string1> <replacement1> [<string2> <replacement2> ...]\n\
	\nExamples:\n\
	  etradeAnonymizer detect statement.pdf\n\
	  etradeAnonymizer replace input.pdf output.pdf \"JAN KOWALSKI\" \"XXXXX XXXXXXXX\""
}

/// Parse arguments and dispatch to detect / replace logic. Returns Ok even
/// for usage errors (prints help) to keep CLI simple.
pub fn run(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    if args.len() < 2 {
        println!("{}", help_text());
        return Ok(());
    }
    match args[1].as_str() {
        "detect" => {
            if args.len() != 3 {
                println!("{}", help_text());
                return Ok(());
            }
            detect::detect_pii(&args[2])
        }
        "replace" => {
            if args.len() < 6 || (args.len() - 4) % 2 != 0 {
                println!("{}", help_text());
                return Ok(());
            }
            let input_path = &args[2];
            let output_path = &args[3];
            let mut replacements: Vec<(String, String)> = Vec::new();
            let mut i = 4;
            while i < args.len() - 1 {
                replacements.push((args[i].clone(), args[i + 1].clone()));
                i += 2;
            }
            replace::replace_mode(input_path, output_path, replacements)
        }
        _ => {
            println!("{}", help_text());
            Ok(())
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Ensure users see warnings and errors by default even when RUST_LOG is not set.
    // If RUST_LOG is provided, simple_logger will respect it; otherwise we default to `warn`.
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "warn");
    }
    simple_logger::SimpleLogger::new().env().init().unwrap();

    let args: Vec<String> = env::args().collect();
    run(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    // Helper to mock args
    fn mock_args(args: &[&str]) -> Vec<String> {
        let mut v = vec!["etradeAnonymizer".to_string()];
        for a in args {
            v.push(a.to_string());
        }
        v
    }

    // Note: These tests require 'anonymizer_data' directory to be present in the working directory
    // when running 'cargo test'.

    #[test]
    fn test_detect_mode() -> Result<(), Box<dyn std::error::Error>> {
        // This test captures stdout, which is tricky in Rust test harness without external crate.
        // However, we can verify it runs without error.

        let sample = "anonymizer_data/sample_statement.pdf";
        if !std::path::Path::new(sample).exists() {
            println!("Skipping test_detect_mode: {} not found", sample);
            return Ok(());
        }

        let args = mock_args(&["detect", sample]);
        run(args)?;
        Ok(())
    }

    #[test]
    fn test_replace_mode() -> Result<(), Box<dyn std::error::Error>> {
        let sample = "anonymizer_data/sample_statement.pdf";
        let expected_pdf = "anonymizer_data/expected_statement.pdf";
        let output_dir = "target/test_outputs";
        let output_pdf = "target/test_outputs/out_sample_statement.pdf";

        if !std::path::Path::new(sample).exists() || !std::path::Path::new(expected_pdf).exists() {
            println!("Skipping test_replace_mode: test data not found");
            return Ok(());
        }

        fs::create_dir_all(output_dir)?;

        // Arguments derived from expected_detect_output.txt content logic in original test
        let args = mock_args(&[
            "replace",
            sample,
            output_pdf,
            "JAN KOWALSKI",
            "XXXXXXXXXXXX",
            "UL. SWIETOKRZYSKA 12",
            "XXXXXXXXXXXXXXXXXXXX",
            "WARSAW 00-916 POLAND",
            "XXXXXXXXXXXXXXXXXXXX",
            "012 - 345678 - 910 -",
            "XXXXXXXXXXXXXXXXXXXX",
            "012-345678-910",
            "XXXXXXXXXXXXXX",
        ]);

        run(args)?;

        let produced = fs::read(output_pdf)?;
        let expected = fs::read(expected_pdf)?;
        assert_eq!(produced, expected, "produced PDF differs from expected");

        // Cleanup
        let _ = fs::remove_file(output_pdf);
        Ok(())
    }
}
