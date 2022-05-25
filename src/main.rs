use chrono;
use chrono::Datelike;
use clap::{App, AppSettings, Arg};

mod de;
mod logging;
mod pdfparser;
mod pl;
mod us;
mod xlsxparser;
use etradeTaxReturnHelper::{Sold_Transaction, Transaction};
use logging::ResultExt;

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

/// Check if all dividends transaction come from the same year
fn verify_dividends_transactions(div_transactions: &Vec<(String, f32, f32)>) -> Result<(), String> {
    let mut trans = div_transactions.iter();
    let (transaction_date, _, _) = match trans.next() {
        Some((x, a, b)) => (x, a, b),
        None => {
            log::info!("No Dividends transactions");
            return Ok(());
        }
    };

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

/// Trade date is when transaction was trigerred. Commission and Fee should
/// be using exchange rate from preceeding day of this date
/// Actual Tax is to be paid from settlement_date
fn reconstruct_sold_transactions(
    sold_transactions: &Vec<(String, i32, f32, f32)>,
    trade_confirmations: &Vec<(String, String, i32, f32, f32, f32, f32, f32)>,
    gains_and_losses: &Vec<(String, String, f32, f32, f32)>,
) -> Result<Vec<(String, String, String, f32, f32, f32)>, String> {
    // Ok What do I need.
    // 1. trade date
    // 2. settlement date
    // 3. date of purchase
    // 4. gross income
    // 5. fee+commission
    // 6. cost cost basis
    let mut detailed_sold_transactions: Vec<(String, String, String, f32, f32, f32)> = vec![];

    // iterate through all sold transactions and update it with needed info
    for (trade_date, _, _, income) in sold_transactions {
        // match trade date and gross with principal and trade date of  trade confirmation

        let (_, settlement_date, _, _, principal, commission, fee, _) = trade_confirmations.iter().find(|(tr_date, _, _, _, _, _, _, net)| tr_date == trade_date && net == income).expect_and_log("Error: Sold transaction detected, but corressponding TRADE confirmation is missing. Please download trade confirmation document.\n");
        let (acquisition_date, _, cost_basis, _, _) = gains_and_losses.iter().find(|(_, tr_date, _,_, principal)| tr_date == trade_date && principal == income).expect_and_log("Error: Sold transaction detected, but corressponding Gain&Losses document is missing. Please download Gain&Losses  XLSX document.\n");
        log::info!("Detailed sold transaction => trade_date: {}, settlement_date: {}, acquisition_date: {}, income: {}, total_fee: {}, cost_basis: {}",trade_date,settlement_date,acquisition_date,income,fee+commission,cost_basis);
        detailed_sold_transactions.push((
            trade_date.clone(),
            settlement_date.clone(),
            acquisition_date.clone(),
            *principal,
            fee + commission,
            *cost_basis,
        ));
    }

    Ok(detailed_sold_transactions)
}

fn create_detailed_div_transactions(
    transactions: Vec<(String, f32, f32)>,
    dates: &std::collections::HashMap<String, Option<(String, f32)>>,
) -> Vec<Transaction> {
    let mut detailed_transactions: Vec<Transaction> = Vec::new();
    transactions
            .iter()
            .for_each(|(transaction_date, gross_us, tax_us)| {
                let (exchange_rate_date, exchange_rate) = dates[transaction_date].clone().unwrap();

            let msg = format!(
                " DIV TRANSACTION date: {}, gross: ${}, tax_us: ${}, exchange_rate: {} , exchange_rate_date: {}",
                chrono::NaiveDate::parse_from_str(&transaction_date, "%m/%d/%y").unwrap().format("%Y-%m-%d"), &gross_us, &tax_us, &exchange_rate, &exchange_rate_date
            )
            .to_owned();

            println!("{}", msg);
            log::info!("{}", msg);


                detailed_transactions.push(Transaction {
                    transaction_date: transaction_date.clone(),
                    gross_us: gross_us.clone(),
                    tax_us: tax_us.clone(),
                    exchange_rate_date: exchange_rate_date,
                    exchange_rate: exchange_rate,
                })
            });
    detailed_transactions
}

//    pub trade_date: String,
//    pub settlement_date: String,
//    pub acquisition_date: String,
//    pub gross_us: f32,
//    pub total_fee: f32,
//    pub cost_basis: f32,
//    pub exchange_rate_trade_date: String,
//    pub exchange_rate_trade: f32,
//    pub exchange_rate_settlement_date: String,
//    pub exchange_rate_settlement: f32,
//    pub exchange_rate_acquisition_date: String,
//    pub exchange_rate_acquisition: f32,
fn create_detailed_sold_transactions(
    transactions: Vec<(String, String, String, f32, f32, f32)>,
    dates: &std::collections::HashMap<String, Option<(String, f32)>>,
) -> Vec<Sold_Transaction> {
    let mut detailed_transactions: Vec<Sold_Transaction> = Vec::new();
    transactions
            .iter()
            .for_each(|(trade_date, settlement_date, acquisition_date, gross, fees, cost_basis)| {
                let (exchange_rate_trade_date, exchange_rate_trade) = dates[trade_date].clone().unwrap();
                let (exchange_rate_settlement_date, exchange_rate_settlement) = dates[settlement_date].clone().unwrap();
                let (exchange_rate_acquisition_date, exchange_rate_acquisition) = dates[acquisition_date].clone().unwrap();

            let msg = format!(
                " SOLD TRANSACTION trade_date: {}, settlement_date: {}, acquisition_date: {}, gross_income: ${},fees: ${}, cost_basis: {}, exchange_rate_trade: {} , exchange_rate_trade_date: {}, exchange_rate_settlement: {} , exchange_rate_settlement_date: {}, exchange_rate_acquisition: {} , exchange_rate_acquisition_date: {}",
                chrono::NaiveDate::parse_from_str(&trade_date, "%m/%d/%y").unwrap().format("%Y-%m-%d"), 
                chrono::NaiveDate::parse_from_str(&settlement_date, "%m/%d/%y").unwrap().format("%Y-%m-%d"), 
                chrono::NaiveDate::parse_from_str(&acquisition_date, "%m/%d/%y").unwrap().format("%Y-%m-%d"), 
                &gross, &fees, &cost_basis, &exchange_rate_trade, &exchange_rate_trade_date,&exchange_rate_settlement, &exchange_rate_settlement_date, &exchange_rate_acquisition, &exchange_rate_acquisition_date,
            )
            .to_owned();

            println!("{}", msg);
            log::info!("{}", msg);

                detailed_transactions.push(Sold_Transaction {
                    trade_date: trade_date.clone(),
                    settlement_date: settlement_date.clone(),
                    acquisition_date: acquisition_date.clone(),
                    gross_us: *gross,
                    total_fee: *fees,
                    cost_basis: *cost_basis,
                    exchange_rate_trade_date: exchange_rate_trade_date,
                    exchange_rate_trade: exchange_rate_trade,
                    exchange_rate_settlement_date: exchange_rate_settlement_date,
                    exchange_rate_settlement: exchange_rate_settlement,
                    exchange_rate_acquisition_date: exchange_rate_acquisition_date,
                    exchange_rate_acquisition: exchange_rate_acquisition,
                })
            });
    detailed_transactions
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

    #[test]
    fn test_dividends_verification_ok() -> Result<(), String> {
        let transactions: Vec<(String, f32, f32)> = vec![
            ("06/01/21".to_string(), 100.0, 25.0),
            ("03/01/21".to_string(), 126.0, 10.0),
        ];
        verify_dividends_transactions(&transactions)
    }

    #[test]
    fn test_create_detailed_div_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, f32, f32)> = vec![
            ("04/11/21".to_string(), 100.0, 25.0),
            ("03/01/21".to_string(), 126.0, 10.0),
        ];

        let mut dates: std::collections::HashMap<String, Option<(String, f32)>> =
            std::collections::HashMap::new();
        dates.insert("03/01/21".to_owned(), Some(("02/28/21".to_owned(), 2.0)));
        dates.insert("04/11/21".to_owned(), Some(("04/10/21".to_owned(), 3.0)));

        let transactions = create_detailed_div_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            vec![
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross_us: 100.0,
                    tax_us: 25.0,
                    exchange_rate_date: "04/10/21".to_string(),
                    exchange_rate: 3.0,
                },
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross_us: 126.0,
                    tax_us: 10.0,
                    exchange_rate_date: "02/28/21".to_string(),
                    exchange_rate: 2.0,
                },
            ]
        );
        Ok(())
    }

    #[test]
    fn test_dividends_verification_empty_ok() -> Result<(), String> {
        let transactions: Vec<(String, f32, f32)> = vec![];
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

    #[test]
    fn test_sold_transaction_reconstruction_ok() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, i32, f32, f32)> = vec![
            ("06/01/21".to_string(), 1, 25.0, 24.8),
            ("03/01/21".to_string(), 2, 10.0, 19.8),
        ];

        let parsed_trade_confirmations: Vec<(String, String, i32, f32, f32, f32, f32, f32)> = vec![
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                1,
                25.0,
                25.0,
                0.01,
                0.01,
                24.8,
            ),
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                1,
                10.0,
                20.0,
                0.01,
                0.01,
                19.8,
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, f32, f32, f32)> = vec![
            (
                "01/01/19".to_string(),
                "06/01/21".to_string(),
                10.0,
                10.0,
                24.8,
            ),
            (
                "01/01/21".to_string(),
                "03/01/21".to_string(),
                20.0,
                20.0,
                19.8,
            ),
        ];

        let detailed_sold_transactions = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_trade_confirmations,
            &parsed_gains_and_losses,
        )?;

        // 1. trade date
        // 2. settlement date
        // 3. date of purchase
        // 4. gross income
        // 5. fee+commission
        // 6. cost cost basis
        assert_eq!(
            detailed_sold_transactions,
            vec![
                (
                    "06/01/21".to_string(),
                    "06/03/21".to_string(),
                    "01/01/19".to_string(),
                    25.0,
                    0.02,
                    10.0
                ),
                (
                    "03/01/21".to_string(),
                    "03/03/21".to_string(),
                    "01/01/21".to_string(),
                    20.0,
                    0.02,
                    20.0
                ),
            ]
        );
        Ok(())
    }
    // TODO : Make negative tests to reconstruction of transaction
}
