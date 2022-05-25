use clap::{App, AppSettings, Arg};

mod de;
mod logging;
mod pdfparser;
mod pl;
mod transactions;
mod us;
mod xlsxparser;
use etradeTaxReturnHelper::{Sold_Transaction, Transaction};
use logging::ResultExt;
use transactions::{
    create_detailed_div_transactions, create_detailed_sold_transactions,
    reconstruct_sold_transactions, verify_dividends_transactions,
};

fn compute_div_taxation(transactions: Vec<Transaction>) -> (f32, f32) {
    // Gross income from dividends in target currency (PLN, EUR etc.)
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

fn compute_sold_taxation(transactions: Vec<Sold_Transaction>) -> (f32, f32) {
    // Gross income from sold stock in target currency (PLN, EUR etc.)
    let gross_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate_settlement * x.gross_us)
        .sum();
    // Cost of income e.g. cost_basis[target currency] + fees[target currency]
    let cost_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate_acquisition * x.cost_basis + x.exchange_rate_trade * x.total_fee)
        .sum();
    (gross_us_pl, cost_us_pl)
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
    let detailed_sold_transactions = reconstruct_sold_transactions(
        &parsed_sold_transactions,
        &parsed_trade_confirmations,
        &parsed_gain_and_losses,
    )
    .expect_and_log("Error reconstructing detailed sold transactions.");

    // 4. Get Exchange rates
    // Gather all trade , settlement and transaction dates into hash map to be passed to
    // get_exchange_rate
    // Hash map : Key(event date) -> (preceeding date, exchange_rate)
    let mut dates: std::collections::HashMap<String, Option<(String, f32)>> =
        std::collections::HashMap::new();
    parsed_div_transactions
        .iter()
        .for_each(|(trade_date, _, _)| {
            if dates.contains_key(trade_date) == false {
                dates.insert(trade_date.clone(), None);
            }
        });
    detailed_sold_transactions.iter().for_each(
        |(trade_date, settlement_date, acquisition_date, _, _, _)| {
            if dates.contains_key(trade_date) == false {
                dates.insert(trade_date.clone(), None);
            }
            if dates.contains_key(settlement_date) == false {
                dates.insert(settlement_date.clone(), None);
            }
            if dates.contains_key(acquisition_date) == false {
                dates.insert(acquisition_date.clone(), None);
            }
        },
    );

    rd.get_exchange_rates(&mut dates)
        .expect_and_log("Error: unable to get exchange rates");

    // Make a detailed_div_transactions
    let transactions = create_detailed_div_transactions(parsed_div_transactions, &dates);
    let sold_transactions = create_detailed_sold_transactions(detailed_sold_transactions, &dates);

    let (gross_div, tax_div) = compute_div_taxation(transactions);
    let (gross_sold, cost_sold) = compute_sold_taxation(sold_transactions);
    rd.present_result(gross_div, tax_div, gross_sold, cost_sold);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{App, Arg, ArgMatches, ErrorKind};

    #[test]
    fn test_exchange_rate_de() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(de::DE {});

        let mut dates: std::collections::HashMap<String, Option<(String, f32)>> =
            std::collections::HashMap::new();
        dates.insert("03/01/21".to_owned(), None);
        rd.get_exchange_rates(&mut dates).unwrap();

        let (exchange_rate_date, exchange_rate) = dates.remove("03/01/21").unwrap().unwrap();
        assert_eq!(
            (exchange_rate_date, exchange_rate),
            ("2021-02-26".to_owned(), 0.82831)
        );
        Ok(())
    }

    #[test]
    fn test_exchange_rate_pl() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});

        let mut dates: std::collections::HashMap<String, Option<(String, f32)>> =
            std::collections::HashMap::new();
        dates.insert("03/01/21".to_owned(), None);
        rd.get_exchange_rates(&mut dates).unwrap();
        let (exchange_rate_date, exchange_rate) = dates.remove("03/01/21").unwrap().unwrap();
        assert_eq!(
            (exchange_rate_date, exchange_rate),
            ("2021-02-26".to_owned(), 3.7247)
        );
        Ok(())
    }

    #[test]
    fn test_exchange_rate_us() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(us::US {});
        let mut dates: std::collections::HashMap<String, Option<(String, f32)>> =
            std::collections::HashMap::new();
        dates.insert("03/01/21".to_owned(), None);
        rd.get_exchange_rates(&mut dates).unwrap();
        let (exchange_rate_date, exchange_rate) = dates.remove("03/01/21").unwrap().unwrap();
        assert_eq!((exchange_rate_date, exchange_rate), ("N/A".to_owned(), 1.0));
        Ok(())
    }

    #[test]
    fn test_simple_div_taxation() -> Result<(), String> {
        // Init Transactions
        let transactions: Vec<Transaction> = vec![Transaction {
            transaction_date: "N/A".to_string(),
            gross_us: 100.0,
            tax_us: 25.0,
            exchange_rate_date: "N/A".to_string(),
            exchange_rate: 4.0,
        }];
        assert_eq!(compute_div_taxation(transactions), (400.0, 100.0));
        Ok(())
    }

    #[test]
    fn test_div_taxation() -> Result<(), String> {
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
            compute_div_taxation(transactions),
            (400.0 + 126.0 * 3.5, 100.0 + 10.0 * 3.5)
        );
        Ok(())
    }

    #[test]
    fn test_simple_sold_taxation() -> Result<(), String> {
        // Init Transactions
        let transactions: Vec<Sold_Transaction> = vec![Sold_Transaction {
            trade_date: "N/A".to_string(),
            settlement_date: "N/A".to_string(),
            acquisition_date: "N/A".to_string(),
            gross_us: 100.0,
            total_fee: 0.02,
            cost_basis: 70.0,
            exchange_rate_trade_date: "N/A".to_string(),
            exchange_rate_trade: 4.0,
            exchange_rate_settlement_date: "N/A".to_string(),
            exchange_rate_settlement: 5.0,
            exchange_rate_acquisition_date: "N/A".to_string(),
            exchange_rate_acquisition: 6.0,
        }];
        assert_eq!(
            compute_sold_taxation(transactions),
            (100.0 * 5.0, 70.0 * 6.0 + 0.02 * 4.0)
        );
        Ok(())
    }

    #[test]
    fn test_sold_taxation() -> Result<(), String> {
        // Init Transactions
        let transactions: Vec<Sold_Transaction> = vec![
            Sold_Transaction {
                trade_date: "N/A".to_string(),
                settlement_date: "N/A".to_string(),
                acquisition_date: "N/A".to_string(),
                gross_us: 100.0,
                total_fee: 0.02,
                cost_basis: 70.0,
                exchange_rate_trade_date: "N/A".to_string(),
                exchange_rate_trade: 4.0,
                exchange_rate_settlement_date: "N/A".to_string(),
                exchange_rate_settlement: 5.0,
                exchange_rate_acquisition_date: "N/A".to_string(),
                exchange_rate_acquisition: 6.0,
            },
            Sold_Transaction {
                trade_date: "N/A".to_string(),
                settlement_date: "N/A".to_string(),
                acquisition_date: "N/A".to_string(),
                gross_us: 10.0,
                total_fee: 0.02,
                cost_basis: 4.0,
                exchange_rate_trade_date: "N/A".to_string(),
                exchange_rate_trade: 1.0,
                exchange_rate_settlement_date: "N/A".to_string(),
                exchange_rate_settlement: 2.0,
                exchange_rate_acquisition_date: "N/A".to_string(),
                exchange_rate_acquisition: 3.0,
            },
        ];
        assert_eq!(
            compute_sold_taxation(transactions),
            (
                100.0 * 5.0 + 10.0 * 2.0,
                70.0 * 6.0 + 4.0 * 3.0 + 0.02 * 1.0 + 0.02 * 4.0
            )
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
