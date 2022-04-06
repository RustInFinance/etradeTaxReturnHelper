use clap::{App, Arg};
use pdf::file::File;
use pdf::primitive::Primitive;

mod pl;
mod us;

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

fn init_logging_infrastructure() {
    // TODO(jczaja): test on windows/macos
    syslog::init(
        syslog::Facility::LOG_USER,
        log::LevelFilter::Debug,
        Some("e-trade-tax-helper"),
    )
    .expect("Error initializing syslog");
}

fn parse_brokerage_statement(pdftoparse: &str) -> Result<(String, f32, f32), String> {
    //2. parsing each pdf
    let mypdffile = File::<Vec<u8>>::open(pdftoparse).unwrap();

    let mut state = ParserState::SearchingDividendEntry;
    let mut transaction_date: String = "N/A".to_string();
    let mut tax_us = 0.0;

    log::info!("Parsing: {}", pdftoparse);
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
                                // If string is "Dividend"
                                if let Primitive::String(actual_string) = &c[0] {
                                    match state {
                                        ParserState::SearchingDividendEntry => {
                                            let rust_string =
                                                actual_string.clone().into_string().unwrap();
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
                                            return Ok((transaction_date, gross_us, tax_us));
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
    Err(format!("Error parsing pdf: {}", pdftoparse))
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

fn main() {
    init_logging_infrastructure();

    let matches = App::new("E-trade tax helper")
        .arg(
            Arg::with_name("residency")
                .long("residency")
                .help("Country of residence e.g. pl , usd ...")
                .value_name("FILE")
                .takes_value(true)
                .default_value("pl"),
        )
        .arg(
            Arg::with_name("pdf documents")
                .help("Brokerage statement PDF files")
                .multiple(true),
        )
        .get_matches();

    let residency = matches
        .value_of("residence")
        .expect("error getting residence value");
    let rd: Box<dyn etradeTaxReturnHelper::Residency> = match residency {
        "pl" => Box::new(pl::PL {}),
        "usd" => Box::new(us::US {}),
        _ => panic!(
            "{}",
            &format!("Error: unimplemented residency: {}", residency)
        ),
    };

    let pdfnames = matches
        .values_of("pdf documents")
        .expect("error getting brokarage statements pdfs names");

    let mut transactions: Vec<Transaction> = Vec::new();
    let args: Vec<String> = std::env::args().collect();

    log::info!("Started e-trade-tax-helper");
    // Start from second one
    for pdfname in pdfnames {
        // 1. Get PDF parsed and attach exchange rate
        log::info!("Processing: {}", pdfname);
        let p = parse_brokerage_statement(&pdfname);

        if let Ok((transaction_date, gross_us, tax_us)) = p {
            let (exchange_rate_date, exchange_rate) = rd
                .get_exchange_rate(&transaction_date)
                .expect("Error getting exchange rate");
            let msg = format!(
                "TRANSACTION date: {}, gross: ${}, tax_us: ${}, exchange_rate: {} pln, exchange_rate_date: {}",
                &transaction_date, &gross_us, &tax_us, &exchange_rate, &exchange_rate_date
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
            Ok(("03/01/21".to_owned(), 574.42, 86.16))
        );
        assert_eq!(
            parse_brokerage_statement("data/example2.pdf"),
            Err(format!("Error parsing pdf: data/example2.pdf"))
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
}

// TODO: cutting out personal info
