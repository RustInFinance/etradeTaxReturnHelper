use clap::{App, AppSettings, Arg};
use std::env;

mod de;
mod logging;
mod pl;
mod us;

mod gui;

use etradeTaxReturnHelper::run_taxation;
use logging::ResultExt;

// TODO: Finish parse_revolut_transactions
// TODO:  async to get currency
// TODO: parse_gain_and_losses  expect ->  ?
// TODO: GUI : choosing residency
// TODO: Drag&Drop to work on MultiBrowser field
// TODO: Change run_taxation for_Each into try_for_Ech if possible
// TODO: taxation of EUR instruments in US

fn create_cmd_line_pattern<'a, 'b>(myapp: App<'a, 'b>) -> App<'a, 'b> {
    myapp
        .arg(
            Arg::with_name("residency")
                .long("residency")
                .help("Country of residence e.g. pl , us ...")
                .value_name("FILE")
                .takes_value(true)
                .default_value("pl"),
        )
        .arg(
            Arg::with_name("financial documents")
                .help("Brokerage statement PDFs  and Gain & Losses xlsx documents\n\nBrokerege statements can be downloaded from:\n\thttps://edoc.etrade.com/e/t/onlinedocs/docsearch?doc_type=stmt\n\nGain&Losses documents can be downloaded from:\n\thttps://us.etrade.com/etx/sp/stockplan#/myAccount/gainsLosses\n")
                .multiple(true)
                .required(true),
        )
}

fn main() {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    logging::init_logging_infrastructure();

    log::info!("Started etradeTaxHelper");
    // If there is no arguments then start GUI
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        #[cfg(feature = "gui")]
        {
            gui::gui::run_gui();
            return;
        }
    }

    let myapp = App::new("etradeTaxHelper ".to_string() + VERSION)
        .setting(AppSettings::ArgRequiredElseHelp);
    let matches = create_cmd_line_pattern(myapp).get_matches_from(wild::args());

    let residency = matches
        .value_of("residency")
        .expect_and_log("error getting residency value");
    let rd: Box<dyn etradeTaxReturnHelper::Residency> = match residency {
        "de" => Box::new(de::DE {}),
        "pl" => Box::new(pl::PL {}),
        "us" => Box::new(us::US {}),
        _ => panic!(
            "{}",
            &format!("Error: unimplemented residency: {}", residency)
        ),
    };

    let pdfnames = matches
        .values_of("financial documents")
        .expect_and_log("error getting brokarage statements pdfs names.\n\nBrokerege statements can be downloaded from:\n\nhttps://edoc.etrade.com/e/t/onlinedocs/docsearch?doc_type=stmt\n\n");

    let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

    let (gross_div, tax_div, gross_sold, cost_sold) = match run_taxation(&rd, pdfnames) {
        Ok((gross_div, tax_div, gross_sold, cost_sold, _, _, _)) => {
            (gross_div, tax_div, gross_sold, cost_sold)
        }
        Err(msg) => panic!("\nError: Unable to compute taxes. \n\nDetails: {msg}"),
    };

    let (presentation, warning) = rd.present_result(gross_div, tax_div, gross_sold, cost_sold);
    presentation.iter().for_each(|x| println!("{x}"));

    if let Some(warn_msg) = warning {
        println!("\n\nWARNING: {warn_msg}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{App, ErrorKind};

    #[test]
    fn test_exchange_rate_de() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(de::DE {});

        let mut dates: std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, f32)>,
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

        assert_eq!(
            (exchange_rate_date, exchange_rate),
            ("2023-02-20".to_owned(), 0.93561)
        );
        Ok(())
    }

    #[test]
    fn test_exchange_rate_pl() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});

        let mut dates: std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, f32)>,
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
            ("2021-02-26".to_owned(), 3.7247)
        );
        Ok(())
    }

    #[test]
    fn test_exchange_rate_us() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(us::US {});

        let mut dates: std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, f32)>,
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

        assert_eq!((exchange_rate_date, exchange_rate), ("N/A".to_owned(), 1.0));
        Ok(())
    }

    #[test]
    fn test_cmdline_de() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = App::new("E-trade tax helper");
        let matches = create_cmd_line_pattern(myapp).get_matches_from_safe(vec![
            "mytest",
            "--residency=de",
            "data/example.pdf",
        ])?;
        let residency = matches.value_of("residency").ok_or(clap::Error {
            message: "Unable to get residency value".to_owned(),
            kind: ErrorKind::InvalidValue,
            info: None,
        })?;
        match residency {
            "de" => return Ok(()),
            _ => clap::Error {
                message: "Wrong residency value".to_owned(),
                kind: ErrorKind::InvalidValue,
                info: None,
            },
        };
        Ok(())
    }

    #[test]
    fn test_cmdline_pl() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = App::new("E-trade tax helper");
        let matches = create_cmd_line_pattern(myapp).get_matches_from_safe(vec![
            "mytest",
            "--residency=pl",
            "data/example.pdf",
        ])?;
        let residency = matches.value_of("residency").ok_or(clap::Error {
            message: "Unable to get residency value".to_owned(),
            kind: ErrorKind::InvalidValue,
            info: None,
        })?;
        match residency {
            "pl" => return Ok(()),
            _ => clap::Error {
                message: "Wrong residency value".to_owned(),
                kind: ErrorKind::InvalidValue,
                info: None,
            },
        };
        Ok(())
    }
    #[test]
    fn test_cmdline_default() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = App::new("E-trade tax helper");
        create_cmd_line_pattern(myapp).get_matches_from_safe(vec!["mytest", "data/example.pdf"])?;
        Ok(())
    }

    #[test]
    fn test_cmdline_us() -> Result<(), clap::Error> {
        // Init Transactions
        let myapp = App::new("E-trade tax helper");
        let matches = create_cmd_line_pattern(myapp).get_matches_from_safe(vec![
            "mytest",
            "--residency=us",
            "data/example.pdf",
        ])?;
        let residency = matches.value_of("residency").ok_or(clap::Error {
            message: "Unable to get residency value".to_owned(),
            kind: ErrorKind::InvalidValue,
            info: None,
        })?;
        match residency {
            "us" => return Ok(()),
            _ => clap::Error {
                message: "Wrong residency value".to_owned(),
                kind: ErrorKind::InvalidValue,
                info: None,
            },
        };
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_dividends_taxation() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = App::new("E-trade tax helper").setting(AppSettings::ArgRequiredElseHelp);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});
        // Check printed values or returned values?
        let matches = create_cmd_line_pattern(myapp).get_matches_from_safe(vec![
            "mytest",
            "data/Brokerage Statement - XXXX0848 - 202202.pdf",
            "data/Brokerage Statement - XXXX0848 - 202203.pdf",
            "data/Brokerage Statement - XXXX0848 - 202204.pdf",
            "data/Brokerage Statement - XXXX0848 - 202205.pdf",
            "data/Brokerage Statement - XXXX0848 - 202206.pdf",
            "data/Brokerage Statement - XXXX0848 - 202209.pdf",
            "data/Brokerage Statement - XXXX0848 - 202211.pdf",
            "data/Brokerage Statement - XXXX0848 - 202212.pdf",
            "data/G&L_Collapsed.xlsx",
        ])?;

        let pdfnames = matches
            .values_of("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames) {
            Ok((gross_div, tax_div, gross_sold, cost_sold, _, _, _)) => {
                assert_eq!(
                    (gross_div, tax_div, gross_sold, cost_sold),
                    (14062.57, 2109.3772, 395.45355, 91.156715)
                );
                Ok(())
            }
            Err(x) => panic!("Error in taxation process: {x}"),
        }
    }

    #[test]
    #[ignore]
    fn test_sold_dividends_taxation() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = App::new("E-trade tax helper").setting(AppSettings::ArgRequiredElseHelp);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});
        let matches = create_cmd_line_pattern(myapp).get_matches_from_safe(vec![
            "mytest",
            "data/Brokerage Statement - XXXX0848 - 202202.pdf",
            "data/Brokerage Statement - XXXX0848 - 202203.pdf",
            "data/Brokerage Statement - XXXX0848 - 202204.pdf",
            "data/Brokerage Statement - XXXX0848 - 202205.pdf",
            "data/G&L_Collapsed.xlsx",
        ])?;
        let pdfnames = matches
            .values_of("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames) {
            Ok((gross_div, tax_div, gross_sold, cost_sold, _, _, _)) => {
                assert_eq!(
                    (gross_div, tax_div, gross_sold, cost_sold),
                    (2930.206, 439.54138, 395.45355, 91.156715)
                );
                Ok(())
            }
            Err(x) => panic!("Error in taxation process"),
        }
    }

    #[test]
    #[ignore]
    fn test_sold_dividends_only_taxation() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = App::new("E-trade tax helper").setting(AppSettings::ArgRequiredElseHelp);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});
        let matches = create_cmd_line_pattern(myapp).get_matches_from_safe(vec![
            "mytest",
            "data/Brokerage Statement - XXXX0848 - 202206.pdf",
        ])?;
        let pdfnames = matches
            .values_of("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        let pdfnames: Vec<String> = pdfnames.map(|x| x.to_string()).collect();

        match etradeTaxReturnHelper::run_taxation(&rd, pdfnames) {
            Ok((gross_div, tax_div, gross_sold, cost_sold, _, _, _)) => {
                assert_eq!(
                    (gross_div, tax_div, gross_sold, cost_sold),
                    (3272.3125, 490.82773, 0.0, 0.0),
                );
                Ok(())
            }
            Err(x) => panic!("Error in taxation process"),
        }
    }
}
