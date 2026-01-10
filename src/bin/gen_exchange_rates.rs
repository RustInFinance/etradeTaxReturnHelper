// SPDX-FileCopyrightText: 2024-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

use clap::{Arg, Command};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;

#[derive(Deserialize)]
struct Kurs {
    no: String,
    effectiveDate: String,
    mid: f64,
}

#[derive(Deserialize)]
struct Tabela {
    table: String,
    currency: String,
    code: String,
    rates: Vec<Kurs>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Exchange {
    EUR(String),
    PLN(String),
    USD(String),
}

fn main() {
    let matches = Command::new("etradeTaxHelper")
        .version("1.1")
        .arg_required_else_help(true)
        .about("Consumes NBP exchange rates and produces rust source code with it")
        .arg(
            Arg::new("input")
                .long("input")
                .value_name("FILE")
                .help("Sets the input files")
                .num_args(1..)
                .action(clap::ArgAction::Append)
                .required(true),
        )
        .get_matches();

    let file_paths = matches
        .get_many::<String>("input")
        .unwrap()
        .cloned()
        .collect::<Vec<_>>();
    let mut kursy_map: HashMap<Exchange, f64> = HashMap::new();

    for file in file_paths {
        let file_content =
            fs::read_to_string(&file).expect(&format!("Unable to read a file: {file}"));

        // Deserializacja JSON do wektora struktur Kurs
        let table: Tabela =
            serde_json::from_str(&file_content).expect("Unable to parse {file} to JSON format");

        // Tworzenie HashMapy
        let kursy = table.rates;
        match table.code.as_str() {
            "USD" => {
                for kurs in kursy {
                    kursy_map.insert(Exchange::USD(kurs.effectiveDate), kurs.mid);
                }
            }
            "EUR" => {
                for kurs in kursy {
                    kursy_map.insert(Exchange::EUR(kurs.effectiveDate), kurs.mid);
                }
            }
            "PLN" => {
                for kurs in kursy {
                    kursy_map.insert(Exchange::PLN(kurs.effectiveDate), kurs.mid);
                }
            }
            _ => {
                panic!("Unsupported currency: {}", table.code);
            }
        }
    }

    // Generowanie pliku .rs z hashmapą
    let mut output_content = String::new();
    output_content.push_str("use std::collections::HashMap;\n\n");
    output_content.push_str("use etradeTaxReturnHelper::Exchange;\n\n");
    output_content.push_str("#[allow(clippy::approx_constant)]\n");

    output_content.push_str("pub fn get_exchange_rates() -> HashMap<Exchange, f64> {\n");
    output_content.push_str("   let mut exchange_rates = HashMap::new();\n");

    for (exchange, kurs) in &kursy_map {
        match exchange {
            Exchange::USD(data) => {
                output_content.push_str(&format!(
                    "  exchange_rates.insert(Exchange::USD(\"{}\".to_string()), {}f64);\n",
                    data, kurs
                ));
            }
            Exchange::EUR(data) => {
                output_content.push_str(&format!(
                    "  exchange_rates.insert(Exchange::EUR(\"{}\".to_string()), {}f64);\n",
                    data, kurs
                ));
            }
            Exchange::PLN(data) => {
                output_content.push_str(&format!(
                    "  exchange_rates.insert(Exchange::PLN(\"{}\".to_string()), {}f64);\n",
                    data, kurs
                ));
            }
        }
    }

    output_content.push_str("   exchange_rates\n");
    output_content.push_str("}\n");
    println!("{output_content}");
}
