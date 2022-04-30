use chrono;
use clap::{App, AppSettings, Arg};
use pdf::file::File;
use pdf::primitive::Primitive;

mod de;
mod logging;
mod pl;
mod us;
use logging::ResultExt;

enum ParserState {
    SearchingDividendEntry,
    SearchingINTCEntry,
    SearchingTaxEntry,
    SearchingGrossEntry,
}

struct Transaction {
    transaction_date: String,
    gross_us: f32,
    tax_us: f32,
    exchange_rate_date: String,
    exchange_rate: f32,
}

fn parse_brokerage_statement(pdftoparse: &str) -> Vec<(String, f32, f32)> {
    //2. parsing each pdf
    let mypdffile = File::<Vec<u8>>::open(pdftoparse)
        .expect_and_log(&format!("Error opening and parsing file: {}", pdftoparse));

    let mut state = ParserState::SearchingDividendEntry;
    let mut transaction_date: String = "N/A".to_string();
    let mut tax_us = 0.0;
    let mut transactions: Vec<(String, f32, f32)> = vec![];

    log::info!("Parsing: {} of {} pages", pdftoparse, mypdffile.num_pages());
    for page in mypdffile.pages() {
        let page = page.unwrap();
        let contents = page.contents.as_ref().unwrap();
        for op in contents.operations.iter() {
            match op.operator.as_ref() {
                "TJ" => {
                    // Text show
                    if op.operands.len() > 0 {
                        //transaction_date = op.operands[0];
                        let a = &op.operands[0];
                        match a {
                            Primitive::Array(c) => {
                                for e in c {
                                    if let Primitive::String(actual_string) = e {
                                        match state {
                                            ParserState::SearchingDividendEntry => {
                                                let rust_string =
                                                    actual_string.clone().into_string().unwrap();
                                                //println!("rust_string: {}", rust_string);
                                                if rust_string == "Dividend" {
                                                    state = ParserState::SearchingINTCEntry;
                                                } else {
                                                    transaction_date = rust_string;
                                                }
                                            }
                                            ParserState::SearchingINTCEntry => {
                                                let rust_string =
                                                    actual_string.clone().into_string().unwrap();
                                                if rust_string == "INTC" {
                                                    state = ParserState::SearchingTaxEntry;
                                                }
                                            }
                                            ParserState::SearchingTaxEntry => {
                                                tax_us = actual_string
                                                    .clone()
                                                    .into_string()
                                                    .unwrap()
                                                    .parse::<f32>()
                                                    .unwrap();
                                                state = ParserState::SearchingGrossEntry
                                            }
                                            ParserState::SearchingGrossEntry => {
                                                let gross_us = actual_string
                                                    .clone()
                                                    .into_string()
                                                    .unwrap()
                                                    .parse::<f32>()
                                                    .unwrap();
                                                state = ParserState::SearchingDividendEntry;
                                                transactions.push((
                                                    transaction_date.clone(),
                                                    gross_us,
                                                    tax_us,
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                            _ => (),
                        }
                    }
                }
                _ => {}
            }
        }
    }
    transactions
}

fn compute_tax(transactions: Vec<Transaction>) -> (f32, f32) {
    // Gross income from dividends in PLN
    let gross_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate * x.gross_us)
        .sum();
    // Tax paind in US in PLN
    let tax_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate * x.tax_us)
        .sum();
    (gross_us_pl, tax_us_pl)
}

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
            Arg::with_name("pdf documents")
                .help("Brokerage statement PDF files")
                .multiple(true)
                .required(true),
        )
}

fn main() {
    logging::init_logging_infrastructure();

    let myapp = App::new("E-trade tax helper").setting(AppSettings::ArgRequiredElseHelp);
    let matches = create_cmd_line_pattern(myapp).get_matches();

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
        .values_of("pdf documents")
        .expect_and_log("error getting brokarage statements pdfs names");

    let mut transactions: Vec<Transaction> = Vec::new();

    log::info!("Started e-trade-tax-helper");
    // Start from second one
    for pdfname in pdfnames {
        // 1. Get PDF parsed and attach exchange rate
        log::info!("Processing: {}", pdfname);
        let parsed_transactions = parse_brokerage_statement(&pdfname);
        for (transaction_date, gross_us, tax_us) in parsed_transactions {
            let (exchange_rate_date, exchange_rate) = rd
                .get_exchange_rate(&transaction_date)
                .expect_and_log("Error getting exchange rate");
            let msg = format!(
                "TRANSACTION date: {}, gross: ${}, tax_us: ${}, exchange_rate: {} , exchange_rate_date: {}",
                chrono::NaiveDate::parse_from_str(&transaction_date, "%m/%d/%y").unwrap().format("%Y-%m-%d"), &gross_us, &tax_us, &exchange_rate, &exchange_rate_date
            )
            .to_owned();

            println!("{}", msg);
            log::info!("{}", msg);
            transactions.push(Transaction {
                transaction_date,
                gross_us,
                tax_us,
                exchange_rate_date,
                exchange_rate,
            });
        }
    }

    let (gross, tax) = compute_tax(transactions);
    rd.present_result(gross, tax);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{App, Arg, ArgMatches, ErrorKind};

    #[test]
    fn test_exchange_rate_de() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(de::DE {});
        assert_eq!(
            rd.get_exchange_rate("03/01/21"),
            Ok(("2021-02-26".to_owned(), 0.82831))
        );
        Ok(())
    }

    #[test]
    fn test_exchange_rate_pl() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});
        assert_eq!(
            rd.get_exchange_rate("03/01/21"),
            Ok(("2021-02-26".to_owned(), 3.7247))
        );
        Ok(())
    }

    #[test]
    fn test_exchange_rate_us() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(us::US {});
        assert_eq!(
            rd.get_exchange_rate("03/01/21"),
            Ok(("N/A".to_owned(), 1.0))
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_parse_brokerage_statement() -> Result<(), String> {
        assert_eq!(
            parse_brokerage_statement("data/example.pdf"),
            vec![("03/01/21".to_owned(), 574.42, 86.16)]
        );
        assert_eq!(parse_brokerage_statement("data/example2.pdf"), vec![]);

        assert_eq!(
            parse_brokerage_statement("data/example3.pdf"),
            vec![
                ("06/01/21".to_owned(), 0.17, 0.03),
                ("06/01/21".to_owned(), 45.87, 6.88)
            ]
        );
        Ok(())
    }

    #[test]
    fn test_simple_computation() -> Result<(), String> {
        // Init Transactions
        let transactions: Vec<Transaction> = vec![Transaction {
            transaction_date: "N/A".to_string(),
            gross_us: 100.0,
            tax_us: 25.0,
            exchange_rate_date: "N/A".to_string(),
            exchange_rate: 4.0,
        }];
        assert_eq!(compute_tax(transactions), (400.0, 100.0));
        Ok(())
    }

    #[test]
    fn test_computation() -> Result<(), String> {
        // Init Transactions
        let transactions: Vec<Transaction> = vec![
            Transaction {
                transaction_date: "N/A".to_string(),
                gross_us: 100.0,
                tax_us: 25.0,
                exchange_rate_date: "N/A".to_string(),
                exchange_rate: 4.0,
            },
            Transaction {
                transaction_date: "N/A".to_string(),
                gross_us: 126.0,
                tax_us: 10.0,
                exchange_rate_date: "N/A".to_string(),
                exchange_rate: 3.5,
            },
        ];
        assert_eq!(
            compute_tax(transactions),
            (400.0 + 126.0 * 3.5, 100.0 + 10.0 * 3.5)
        );
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
}
