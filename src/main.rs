use chrono;
use clap::{App, AppSettings, Arg};
use pdf::file::File;
use pdf::primitive::Primitive;

mod de;
mod logging;
mod pl;
mod us;
use etradeTaxReturnHelper::Transaction;
use logging::ResultExt;

enum TransactionType {
    Sold,
    Dividends,
}

enum ParserState {
    SearchingTransactionEntry,
    ProcessingTransaction(TransactionType),
}

pub trait Entry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString);
    fn getf32(&self) -> Option<f32> {
        None
    }
    fn geti32(&self) -> Option<i32> {
        None
    }
    fn getstring(&self) -> Option<String> {
        None
    }

    fn is_pattern(&self) -> bool {
        false
    }
}

struct F32Entry {
    pub val: f32,
}

impl Entry for F32Entry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) {
        let mystr = pstr
            .clone()
            .into_string()
            .expect(&format!("Error parsing : {:#?} to f32", pstr));
        self.val = mystr
            .parse::<f32>()
            .expect(&format!("Error parsing : {} to f32", mystr));
    }
    fn getf32(&self) -> Option<f32> {
        Some(self.val)
    }
}

struct I32Entry {
    pub val: i32,
}

impl Entry for I32Entry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) {
        let mystr = pstr
            .clone()
            .into_string()
            .expect(&format!("Error parsing : {:#?} to f32", pstr));
        self.val = mystr
            .parse::<i32>()
            .expect(&format!("Error parsing : {} to f32", mystr));
    }
    fn geti32(&self) -> Option<i32> {
        Some(self.val)
    }
}

struct StringEntry {
    pub val: String,
    pub pattern: String,
}

impl Entry for StringEntry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) {
        self.val = pstr
            .clone()
            .into_string()
            .expect(&format!("Error parsing : {:#?} to f32", pstr));
    }
    fn getstring(&self) -> Option<String> {
        Some(self.val.clone())
    }
    fn is_pattern(&self) -> bool {
        self.pattern == self.val
    }
}
fn parse_brokerage_statement(
    pdftoparse: &str,
) -> (Vec<(String, f32, f32)>, Vec<(String, i32, f32, f32)>) {
    //2. parsing each pdf
    let mypdffile = File::<Vec<u8>>::open(pdftoparse)
        .expect_and_log(&format!("Error opening and parsing file: {}", pdftoparse));

    let mut state = ParserState::SearchingTransactionEntry;
    let mut sequence: std::collections::VecDeque<Box<dyn Entry>> =
        std::collections::VecDeque::new();
    let mut processed_sequence: Vec<Box<dyn Entry>> = vec![];

    let mut transaction_date: String = "N/A".to_string();
    let mut div_transactions: Vec<(String, f32, f32)> = vec![];
    let mut sold_transactions: Vec<(String, i32, f32, f32)> = vec![];

    // TODO: how to distinguish brokerage statement from Trade confirmation
    // TODO: Move parsing to separate module
    // TODO: Implement trade confirmation missing info

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
                                            ParserState::SearchingTransactionEntry => {
                                                let rust_string =
                                                    actual_string.clone().into_string().unwrap();
                                                //println!("rust_string: {}", rust_string);
                                                if rust_string == "Dividend" {
                                                    sequence.push_back(Box::new(StringEntry {
                                                        val: String::new(),
                                                        pattern: "INTC".to_owned(),
                                                    })); // INTC
                                                    sequence
                                                        .push_back(Box::new(F32Entry { val: 0.0 })); // Tax Entry
                                                    sequence
                                                        .push_back(Box::new(F32Entry { val: 0.0 })); // Income Entry
                                                    state = ParserState::ProcessingTransaction(
                                                        TransactionType::Dividends,
                                                    );
                                                } else if rust_string == "Sold" {
                                                    sequence
                                                        .push_back(Box::new(I32Entry { val: 0 })); // Quantity
                                                    sequence
                                                        .push_back(Box::new(F32Entry { val: 0.0 })); // Price
                                                    sequence
                                                        .push_back(Box::new(F32Entry { val: 0.0 })); // Amount Sold
                                                    state = ParserState::ProcessingTransaction(
                                                        TransactionType::Sold,
                                                    );
                                                } else {
                                                    //if this is date then store it
                                                    if chrono::NaiveDate::parse_from_str(
                                                        &rust_string,
                                                        "%m/%d/%y",
                                                    )
                                                    .is_ok()
                                                    {
                                                        transaction_date = rust_string.clone();
                                                    }
                                                }
                                            }
                                            ParserState::ProcessingTransaction(
                                                transaction_type,
                                            ) => {
                                                // So process transaction element and store it in SOLD
                                                // or DIV
                                                let possible_obj = sequence.pop_front();
                                                match possible_obj {
                                                    // Move executed parser objects into Vector
                                                    // attach only i32 and f32 elements to
                                                    // processed queue
                                                    Some(mut obj) => {
                                                        obj.parse(actual_string);
                                                        // attach to sequence the same string parser if pattern is not met
                                                        if obj.getstring().is_some() {
                                                            if obj.is_pattern() == false {
                                                                sequence.push_front(obj);
                                                            }
                                                        } else {
                                                            processed_sequence.push(obj);
                                                        }
                                                        state = ParserState::ProcessingTransaction(
                                                            transaction_type,
                                                        );
                                                    }

                                                    // In nothing more to be done then just extract
                                                    // parsed data from paser objects
                                                    None => {
                                                        state =
                                                            ParserState::SearchingTransactionEntry;
                                                        let mut transaction =
                                                            processed_sequence.iter();
                                                        match transaction_type {
                                                            TransactionType::Dividends => {
                                                                // For Dividends first is couple of strings
                                                                // which we will skip
                                                                let tax_us = transaction.next().unwrap().getf32().expect_and_log("Processing of Dividend transaction went wrong");
                                                                let gross_us = transaction.next().unwrap().getf32().expect_and_log("Processing of Dividend transaction went wrong");
                                                                div_transactions.push((
                                                                    transaction_date.clone(),
                                                                    gross_us,
                                                                    tax_us,
                                                                ));
                                                            }
                                                            TransactionType::Sold => {
                                                                let quantity =  transaction.next().unwrap().geti32().expect_and_log("Processing of Sold transaction went wrong");
                                                                let price = transaction.next().unwrap().getf32().expect_and_log("Processing of Sold transaction went wrong");
                                                                let amount_sold =  transaction.next().unwrap().getf32().expect_and_log("Prasing of Sold transaction went wrong");
                                                                //println!("SOLD TRANSACTION date: {}, quantity : {} price: {}, amount_sold: {}",transaction_date, quantity, price, amount_sold);
                                                                sold_transactions.push((
                                                                    transaction_date.clone(),
                                                                    quantity,
                                                                    price,
                                                                    amount_sold,
                                                                ));
                                                            }
                                                        }
                                                        processed_sequence.clear();
                                                        sequence.clear();
                                                    }
                                                }
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
    (div_transactions, sold_transactions)
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

    log::info!("Started etradeTaxHelper");

    let mut parsed_div_transactions: Vec<(String, f32, f32)> = vec![];
    let mut parsed_sold_transactions: Vec<(String, i32, f32, f32)> = vec![];
    // 1. Parse PDF documents to get list of transactions
    pdfnames.for_each(|x| {
        let (mut div_t, mut sold_t) = parse_brokerage_statement(x);
        parsed_div_transactions.append(&mut div_t);
        parsed_sold_transactions.append(&mut sold_t)
    });
    // 2. Get Exchange rates
    let transactions = rd
        .get_exchange_rates(parsed_div_transactions)
        .expect_and_log("Error: unable to get exchange rates");
    transactions.iter().for_each(|x| {
            let msg = format!(
                "TRANSACTION date: {}, gross: ${}, tax_us: ${}, exchange_rate: {} , exchange_rate_date: {}",
                chrono::NaiveDate::parse_from_str(&x.transaction_date, "%m/%d/%y").unwrap().format("%Y-%m-%d"), &x.gross_us, &x.tax_us, &x.exchange_rate, &x.exchange_rate_date
            )
            .to_owned();

            println!("{}", msg);
            log::info!("{}", msg);
            });

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

        let transactions = rd
            .get_exchange_rates(vec![("03/01/21".to_owned(), 0.0, 0.0)])
            .unwrap();
        assert_eq!(
            (
                &transactions[0].exchange_rate_date,
                transactions[0].exchange_rate
            ),
            (&"2021-02-26".to_owned(), 0.82831)
        );
        Ok(())
    }

    #[test]
    fn test_exchange_rate_pl() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});

        let transactions = rd
            .get_exchange_rates(vec![("03/01/21".to_owned(), 0.0, 0.0)])
            .unwrap();
        assert_eq!(
            (
                &transactions[0].exchange_rate_date,
                transactions[0].exchange_rate
            ),
            (&"2021-02-26".to_owned(), 3.7247)
        );
        Ok(())
    }

    #[test]
    fn test_exchange_rate_us() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(us::US {});
        let transactions = rd
            .get_exchange_rates(vec![("03/01/21".to_owned(), 0.0, 0.0)])
            .unwrap();
        assert_eq!(
            (
                &transactions[0].exchange_rate_date,
                transactions[0].exchange_rate
            ),
            (&"N/A".to_owned(), 1.0)
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_parse_brokerage_statement() -> Result<(), String> {
        assert_eq!(
            parse_brokerage_statement("data/example.pdf"),
            (vec![("03/01/21".to_owned(), 574.42, 86.16)], vec![])
        );
        assert_eq!(
            parse_brokerage_statement("data/example2.pdf"),
            (vec![], vec![])
        );

        assert_eq!(
            parse_brokerage_statement("data/example3.pdf"),
            (
                vec![
                    ("06/01/21".to_owned(), 0.17, 0.03),
                    ("06/01/21".to_owned(), 45.87, 6.88)
                ],
                vec![]
            )
        );
        assert_eq!(
            parse_brokerage_statement("data/example4.pdf"),
            (vec![], vec![("04/13/22".to_owned(), -1, 46.92, 46.90)])
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
