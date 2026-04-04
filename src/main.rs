// SPDX-FileCopyrightText: 2022-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

#![debugger_visualizer(natvis_file = "../rust_decimal.natvis")]

use clap::{Arg, Command};
use std::env;

mod de;
mod logging;
mod nbp;
mod pl;
mod us;

mod gui;

use etradeTaxReturnHelper::run_taxation;
use etradeTaxReturnHelper::TaxCalculationResult;
use logging::ResultExt;

// TODO: check if Tax from Terna company taken by IT goverment was taken into account
// TODO: Extend structure of TaxCalculationResult with country
// TODO: Make parsing of PDF start from first page not second so then reproduction of problem
// require one page not two
// TODO: remove support for account statement of investment account of revolut
// TODO: When there is no proxy (on intel account) there are problems (UT do not work
// getting_Exchange_rate)
// TODO: Make a parsing of incomplete date
// TODO:  async to get currency
// TODO: make UT using rounded vlaues of f32
// TODO: parse_gain_and_losses  expect ->  ?
// TODO: GUI : choosing residency
// TODO: Drag&Drop to work on MultiBrowser field
// TODO: taxation of EUR instruments in US

fn create_cmd_line_pattern(myapp: Command) -> Command {
    myapp
        .arg(
            Arg::new("residency")
                .long("residency")
                .help("Country of residence e.g. pl , us ...")
                .value_name("FILE")
                .default_value("pl"),
        )
        .arg(
            Arg::new("financial documents")
                .help("Account statement PDFs  and Gain & Losses xlsx documents\n\nAccount statements can be downloaded from:\n\thttps://edoc.etrade.com/e/t/onlinedocs/docsearch?doc_type=stmt\n\nGain&Losses documents can be downloaded from:\n\thttps://us.etrade.com/etx/sp/stockplan#/myAccount/gainsLosses\n")
                .num_args(1..)
                .required(true),
        )
        .arg(
            Arg::new("per-company")
                .long("per-company")
                .help("Enable per-company mode")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("multiyear")
                .long("multiyear")
                .help("Allow processing documents across more than year")
                .action(clap::ArgAction::SetTrue)
        )
        .arg(
            Arg::new("round-per-transaction")
                .long("round-per-transaction")
                .help("Round each FX-converted amount to grosz before summing (off by default)")
                .action(clap::ArgAction::SetTrue)
        )
}

fn configure_dataframes_format() {
    // Make sure to show all raws
    if std::env::var("POLARS_FMT_MAX_ROWS").is_err() {
        std::env::set_var("POLARS_FMT_MAX_ROWS", "-1")
    }
}

fn main() {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    logging::init_logging_infrastructure();
    configure_dataframes_format();

    log::info!("Started etradeTaxHelper");
    // If there is no arguments then start GUI
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        #[cfg(feature = "gui")]
        {
            gui::run_gui();
            return;
        }
    }

    let myapp = Command::new("etradeTaxHelper")
        .version(VERSION)
        .arg_required_else_help(true);
    let matches = create_cmd_line_pattern(myapp).get_matches_from(wild::args());

    let residency = matches
        .get_one::<String>("residency")
        .expect_and_log("error getting residency value");
    let rd: Box<dyn etradeTaxReturnHelper::Residency> = match residency.as_str() {
        "de" => Box::new(de::DE {}),
        "pl" => Box::new(pl::PL {}),
        "us" => Box::new(us::US {}),
        _ => panic!(
            "{}",
            &format!("Error: unimplemented residency: {}", residency)
        ),
    };

    let pdfnames = matches
        .get_many::<String>("financial documents")
        .expect_and_log("error getting brokarage statements pdfs names.\n\nBrokerege statements can be downloaded from:\n\nhttps://edoc.etrade.com/e/t/onlinedocs/docsearch?doc_type=stmt\n\n");

    let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

    let TaxCalculationResult {
        gross_interests,
        gross_div,
        tax: tax_div,
        gross_sold,
        cost_sold,
        missing_trade_confirmations_warning,
        ..
    } = match run_taxation(
        &rd,
        pdfnames,
        matches.get_flag("per-company"),
        matches.get_flag("multiyear"),
        matches.get_flag("round-per-transaction"),
    ) {
        Ok(res) => res,
        Err(msg) => panic!("\nError: Unable to compute taxes. \n\nDetails: {msg}"),
    };

    let (presentation, warning) =
        rd.present_result(gross_interests, gross_div, tax_div, gross_sold, cost_sold);
    presentation.iter().for_each(|x| println!("{x}"));

    if let Some(warn_msg) = warning {
        println!("\n\nWARNING: {warn_msg}");
    }

    if let Some(tc_warning) = missing_trade_confirmations_warning {
        println!("\n\n{tc_warning}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Command;
    use rust_decimal::dec;
    use rust_decimal::Decimal;

    #[test]
    fn test_exchange_rate_de() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(de::DE {});

        let mut dates: std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, Decimal)>,
        > = std::collections::HashMap::new();

        dates.insert(
            etradeTaxReturnHelper::Exchange::USD("02/21/23".to_owned()),
            None,
        );

        rd.get_exchange_rates(&mut dates)?;

        let (exchange_rate_date, exchange_rate) = dates
            [&etradeTaxReturnHelper::Exchange::USD("02/21/23".to_owned())]
            .clone()
            .unwrap();

        assert_eq!(exchange_rate_date, "2023-02-20");
        assert_eq!(exchange_rate, dec!(0.9368559115608019486602960465));
        Ok(())
    }

    #[test]
    fn test_exchange_rate_pl() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});

        let mut dates: std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, Decimal)>,
        > = std::collections::HashMap::new();

        dates.insert(
            etradeTaxReturnHelper::Exchange::USD("03/01/21".to_owned()),
            None,
        );

        rd.get_exchange_rates(&mut dates)?;

        let (exchange_rate_date, exchange_rate) = dates
            [&etradeTaxReturnHelper::Exchange::USD("03/01/21".to_owned())]
            .clone()
            .unwrap();

        assert_eq!(
            (exchange_rate_date, exchange_rate),
            ("2021-02-26".to_owned(), dec!(3.7247))
        );
        Ok(())
    }

    #[test]
    fn test_exchange_rate_us() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(us::US {});

        let mut dates: std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, Decimal)>,
        > = std::collections::HashMap::new();

        dates.insert(
            etradeTaxReturnHelper::Exchange::USD("03/01/21".to_owned()),
            None,
        );

        rd.get_exchange_rates(&mut dates)?;

        let (exchange_rate_date, exchange_rate) = dates
            [&etradeTaxReturnHelper::Exchange::USD("03/01/21".to_owned())]
            .clone()
            .unwrap();

        assert_eq!(
            (exchange_rate_date, exchange_rate),
            ("N/A".to_owned(), Decimal::ONE)
        );
        Ok(())
    }

    #[test]
    fn test_cmdline_de() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = Command::new("E-trade tax helper");
        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "--residency=de",
            "data/example.pdf",
        ]);
        let residency = matches
            .get_one::<String>("residency")
            .ok_or(clap::error::Error::new(
                clap::error::ErrorKind::InvalidValue,
            ))?;
        match residency.as_str() {
            "de" => return Ok(()),
            _ => clap::error::Error::<clap::error::DefaultFormatter>::new(
                clap::error::ErrorKind::InvalidValue,
            ),
        };
        Ok(())
    }
    #[test]
    fn test_cmdline_per_company() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = Command::new("E-trade tax helper");
        let matches =
            create_cmd_line_pattern(myapp).get_matches_from(vec!["mytest", "data/example.pdf"]);
        let per_company = matches.get_flag("per-company");
        match per_company {
            false => (),
            true => {
                return Err(clap::error::Error::<clap::error::DefaultFormatter>::new(
                    clap::error::ErrorKind::InvalidValue,
                ))
            }
        };
        let myapp = Command::new("E-trade tax helper");
        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "--per-company",
            "data/example.pdf",
        ]);
        let per_company = matches.get_flag("per-company");
        match per_company {
            true => (),
            false => {
                return Err(clap::error::Error::<clap::error::DefaultFormatter>::new(
                    clap::error::ErrorKind::InvalidValue,
                ))
            }
        };
        Ok(())
    }

    #[test]
    fn test_cmdline_multiyear() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = Command::new("E-trade tax helper");
        let matches =
            create_cmd_line_pattern(myapp).get_matches_from(vec!["mytest", "data/example.pdf"]);
        let multiyear = matches.get_flag("multiyear");
        match multiyear {
            false => (),
            true => {
                return Err(clap::error::Error::<clap::error::DefaultFormatter>::new(
                    clap::error::ErrorKind::InvalidValue,
                ))
            }
        };
        let myapp = Command::new("E-trade tax helper");
        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "--multiyear",
            "data/example.pdf",
        ]);
        let multiyear = matches.get_flag("multiyear");
        match multiyear {
            true => (),
            false => {
                return Err(clap::error::Error::<clap::error::DefaultFormatter>::new(
                    clap::error::ErrorKind::InvalidValue,
                ))
            }
        };
        Ok(())
    }

    #[test]
    fn test_cmdline_pl() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = Command::new("E-trade tax helper");
        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "--residency=pl",
            "data/example.pdf",
        ]);
        let residency = matches
            .get_one::<String>("residency")
            .ok_or(clap::error::Error::new(
                clap::error::ErrorKind::InvalidValue,
            ))?;
        match residency.as_str() {
            "pl" => return Ok(()),
            _ => clap::error::Error::<clap::error::DefaultFormatter>::new(
                clap::error::ErrorKind::InvalidValue,
            ),
        };
        Ok(())
    }
    #[test]
    fn test_cmdline_default() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = Command::new("E-trade tax helper");
        create_cmd_line_pattern(myapp).get_matches_from(vec!["mytest", "data/example.pdf"]);
        Ok(())
    }

    #[test]
    fn test_cmdline_us() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = Command::new("E-trade tax helper");
        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "--residency=us",
            "data/example.pdf",
        ]);
        let residency = matches
            .get_one::<String>("residency")
            .ok_or(clap::error::Error::new(
                clap::error::ErrorKind::InvalidValue,
            ))?;
        match residency.as_str() {
            "us" => return Ok(()),
            _ => clap::error::Error::<clap::error::DefaultFormatter>::new(
                clap::error::ErrorKind::InvalidValue,
            ),
        };
        Ok(())
    }

    #[test]
    fn test_unrecognized_file_taxation() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only

        let myapp = Command::new("etradeTaxHelper").arg_required_else_help(true);

        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});
        // Check printed values or returned values?
        let matches = create_cmd_line_pattern(myapp)
            .get_matches_from(vec!["mytest", "unrecognized_file.txt"]);

        let pdfnames = matches
            .get_many::<String>("financial documents")
            .expect_and_log("error getting financial documents names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames, false, false, false) {
            Ok(_) => panic!("Expected an error from run_taxation, but got Ok"),
            Err(_) => Ok(()), // Expected error, test passes
        }
    }

    #[test]
    fn test_revolut_dividends_pln() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = Command::new("etradeTaxHelper").arg_required_else_help(true);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});

        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "revolut_data/trading-pnl-statement_2024-01-01_2024-08-04_pl-pl_8e8783.csv",
        ]);
        let pdfnames = matches
            .get_many::<String>("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames, false, false, false) {
            Ok(TaxCalculationResult {
                gross_interests,
                gross_div,
                tax: tax_div,
                gross_sold,
                cost_sold,
                ..
            }) => {
                assert_eq!(
                    (gross_interests, gross_div, tax_div, gross_sold, cost_sold),
                    (dec!(0), dec!(6331.29), dec!(871.18), dec!(0), dec!(0)),
                );
                Ok(())
            }
            Err(x) => panic!("Error in taxation process: {x}"),
        }
    }

    #[test]
    fn test_revolut_sold_and_dividends() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = Command::new("etradeTaxHelper").arg_required_else_help(true);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});

        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "revolut_data/trading-pnl-statement_2022-11-01_2024-09-01_pl-pl_e989f4.csv",
        ]);
        let pdfnames = matches
            .get_many::<String>("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames, false, false, false) {
            Ok(TaxCalculationResult {
                gross_interests,
                gross_div,
                tax: tax_div,
                gross_sold,
                cost_sold,
                ..
            }) => {
                assert_eq!(
                    (gross_interests, gross_div, tax_div, gross_sold, cost_sold),
                    (
                        dec!(0),
                        dec!(9142.32),
                        dec!(1207.08),
                        dec!(22988.62),
                        dec!(20163.50)
                    ),
                );
                Ok(())
            }
            Err(x) => panic!("Error in taxation process: {x}"),
        }
    }

    #[test]
    fn test_revolut_sold_and_dividends_round_per_transaction() -> Result<(), clap::Error> {
        let myapp = Command::new("etradeTaxHelper").arg_required_else_help(true);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});

        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "revolut_data/trading-pnl-statement_2022-11-01_2024-09-01_pl-pl_e989f4.csv",
        ]);
        let pdfnames = matches
            .get_many::<String>("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames, false, false, true) {
            Ok(TaxCalculationResult {
                gross_interests,
                gross_div,
                tax: tax_div,
                gross_sold,
                cost_sold,
                ..
            }) => {
                assert_eq!(
                    (gross_interests, gross_div, tax_div, gross_sold, cost_sold),
                    (
                        dec!(0),
                        dec!(9142.32),
                        dec!(1207.08),
                        dec!(22988.62),
                        dec!(20163.50)
                    ),
                );
                Ok(())
            }
            Err(x) => panic!("Error in taxation process: {x}"),
        }
    }

    #[test]
    fn test_revolut_interests_taxation_pln() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = Command::new("etradeTaxHelper").arg_required_else_help(true);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});

        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "revolut_data/Revolut_30cze2023_27lis2023.csv",
        ]);
        let pdfnames = matches
            .get_many::<String>("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames, false, false, false) {
            Ok(TaxCalculationResult {
                gross_interests,
                gross_div,
                tax: tax_div,
                gross_sold,
                cost_sold,
                ..
            }) => {
                assert_eq!(
                    (gross_interests, gross_div, tax_div, gross_sold, cost_sold),
                    (dec!(86.93), dec!(0), dec!(0), dec!(0), dec!(0)),
                );
                Ok(())
            }
            Err(x) => panic!("Error in taxation process: {x}"),
        }
    }

    #[test]
    #[ignore]
    fn test_sold_dividends_interests_taxation() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = Command::new("etradeTaxHelper").arg_required_else_help(true);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});
        let matches = create_cmd_line_pattern(myapp).get_matches_from(vec![
            "mytest",
            "etrade_data_2025/ClientStatements_010226.pdf",
            "etrade_data_2025/G&L_Collapsed.xlsx",
        ]);
        let pdfnames = matches
            .get_many::<String>("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames, false, false, false) {
            Ok(TaxCalculationResult {
                gross_interests,
                gross_div,
                tax: tax_div,
                gross_sold,
                cost_sold,
                ..
            }) => {
                assert_eq!(
                    (gross_interests, gross_div, tax_div, gross_sold, cost_sold),
                    (
                        dec!(0),
                        dec!(219.34755),
                        dec!(0.0),
                        dec!(89845.65),
                        dec!(44369.938)
                    ),
                );
                Ok(())
            }
            Err(x) => panic!("Error in taxation process: {x}"),
        }
    }

    #[test]
    #[ignore]
    fn test_interest_adjustment_taxation() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = Command::new("etradeTaxHelper").arg_required_else_help(true);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});
        let matches = create_cmd_line_pattern(myapp)
            .get_matches_from(vec!["mytest", "data/example-interest-adj.pdf"]);
        let pdfnames = matches
            .get_many::<String>("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames, false, false, false) {
            Ok(TaxCalculationResult {
                gross_interests,
                gross_div,
                tax: tax_div,
                gross_sold,
                cost_sold,
                ..
            }) => {
                assert_eq!(
                    (gross_interests, gross_div, tax_div, gross_sold, cost_sold),
                    (dec!(0.66164804), dec!(0), dec!(0.0), dec!(0.0), dec!(0.0)),
                );
                Ok(())
            }
            Err(x) => panic!("Error in taxation process: {x}"),
        }
    }
}
