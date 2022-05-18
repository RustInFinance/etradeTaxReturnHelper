use chrono;
use chrono::Datelike;
use clap::{App, AppSettings, Arg};

mod de;
mod logging;
mod pdfparser;
mod pl;
mod us;
mod xlsxparser;
use etradeTaxReturnHelper::Transaction;
use logging::ResultExt;

fn compute_tax(transactions: Vec<Transaction>) -> (f32, f32) {
    // Gross income from dividends in PLN
    let gross_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate * x.gross_us)
        .sum();
    // Tax paid in US in PLN
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
            Arg::with_name("financial documents")
                .help("Brokerage statement PDFs, Trade confirmation PDFs and Gain & Losses xlsx documents")
                .multiple(true)
                .required(true),
        )
}

/// Check if all dividends transaction come from the same year
fn verify_dividends_transactions(div_transactions: &Vec<(String, f32, f32)>) -> Result<(), String> {
    let mut trans = div_transactions.iter();
    let (transaction_date, _, _) = trans.next().expect_and_log("No Dividends transactions");
    let transaction_year = chrono::NaiveDate::parse_from_str(&transaction_date, "%m/%d/%y")
        .unwrap()
        .year();
    let mut verification: Result<(), String> = Ok(());
    trans.for_each(|(tr_date, _, _)| {
        let tr_year = chrono::NaiveDate::parse_from_str(&tr_date, "%m/%d/%y")
            .unwrap()
            .year();
        if tr_year != transaction_year {
            let msg: &str =
                "WARNING! Brokerage statements are related to different years. Was it intentional?";
            verification = Err(msg.to_owned());
        }
    });
    verification
}

fn main() {
    logging::init_logging_infrastructure();

    let myapp = App::new("E-trade tax helper").setting(AppSettings::ArgRequiredElseHelp);
    let matches = create_cmd_line_pattern(myapp).get_matches();

    log::info!("Started etradeTaxHelper");

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
        .expect_and_log("error getting brokarage statements pdfs names");

    let mut parsed_div_transactions: Vec<(String, f32, f32)> = vec![];
    let mut parsed_sold_transactions: Vec<(String, i32, f32, f32)> = vec![];
    let mut parsed_trade_confirmations: Vec<(String, String, i32, f32, f32, f32, f32, f32)> =
        vec![];
    let mut parsed_gain_and_losses: Vec<(String, String, f32, f32, f32)> = vec![];

    // 1. Parse PDF and XLSX documents to get list of transactions
    pdfnames.for_each(|x| {
        // If name contains .pdf then parse as pdf
        // if name contains .xlsx then parse as spreadsheet
        if x.contains(".pdf") {
            let (mut div_t, mut sold_t, mut trade_t) = pdfparser::parse_brokerage_statement(x);
            parsed_div_transactions.append(&mut div_t);
            parsed_sold_transactions.append(&mut sold_t);
            parsed_trade_confirmations.append(&mut trade_t);
        } else {
            parsed_gain_and_losses.append(&mut xlsxparser::parse_gains_and_losses(x));
        }
    });
    // 2. Verify Transactions
    match verify_dividends_transactions(&parsed_div_transactions) {
        Ok(()) => log::info!("Dividends transactions are consistent"),
        Err(msg) => {
            println!("{}", msg);
            log::warn!("{}", msg);
        }
    }

    // 3. Verify and create full sold transactions info needed for TAX purposes

    // TODO: Implement trade confirmation missing info
    // 4. Get Exchange rates
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

    #[test]
    fn test_dividends_verification_ok() -> Result<(), String> {
        let transactions: Vec<(String, f32, f32)> = vec![
            ("06/01/21".to_string(), 100.0, 25.0),
            ("03/01/21".to_string(), 126.0, 10.0),
        ];
        verify_dividends_transactions(&transactions)
    }

    #[test]
    fn test_dividends_verification_fail() -> Result<(), String> {
        let transactions: Vec<(String, f32, f32)> = vec![
            ("04/11/22".to_string(), 100.0, 25.0),
            ("03/01/21".to_string(), 126.0, 10.0),
        ];
        assert!(verify_dividends_transactions(&transactions).is_err());
        Ok(())
    }
}
