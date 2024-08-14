use chrono;
use chrono::Datelike;

pub use crate::logging::ResultExt;
use crate::{SoldTransaction, Transaction};

/// Check if all interests rate transactions come from the same year
pub fn verify_interests_transactions(
    interests_transactions: &Vec<(String, f32)>,
) -> Result<(), String> {
    let mut trans = interests_transactions.iter();
    let (transaction_date, _) = match trans.next() {
        Some((x, a)) => (x, a),
        None => {
            log::info!("No interests transactions");
            return Ok(());
        }
    };

    let transaction_year = chrono::NaiveDate::parse_from_str(&transaction_date, "%m/%d/%y")
        .unwrap()
        .year();
    let mut verification: Result<(), String> = Ok(());
    trans.for_each(|(tr_date, _)| {
        let tr_year = chrono::NaiveDate::parse_from_str(&tr_date, "%m/%d/%y")
            .unwrap()
            .year();
        if tr_year != transaction_year {
            let msg: &str = "Error:  Brokerage statements are related to different years!";
            verification = Err(msg.to_owned());
        }
    });
    verification
}

/// Check if all dividends transaction come from the same year
pub fn verify_dividends_transactions(
    div_transactions: &Vec<(String, f32, f32)>,
) -> Result<(), String> {
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
            let msg: &str = "Error:  Brokerage statements are related to different years!";
            verification = Err(msg.to_owned());
        }
    });
    verification
}

/// Trade date is when transaction was trigerred.
/// fees and commission are applied at the moment of settlement date so
/// we ignore those and use net income rather than principal
/// Actual Tax is to be paid from settlement_date
pub fn reconstruct_sold_transactions(
    sold_transactions: &Vec<(String, String, f32, f32, f32)>,
    gains_and_losses: &Vec<(String, String, f32, f32, f32)>,
) -> Result<Vec<(String, String, String, f32, f32)>, String> {
    // Ok What do I need.
    // 1. trade date
    // 2. settlement date
    // 3. date of purchase
    // 4. gross income
    // 5. cost cost basis
    let mut detailed_sold_transactions: Vec<(String, String, String, f32, f32)> = vec![];

    if sold_transactions.len() > 0 && gains_and_losses.is_empty() {
        return Err("\n\nERROR: Sold transaction detected, but corressponding Gain&Losses document is missing. Please download Gain&Losses  XLSX document at:\n
            https://us.etrade.com/etx/sp/stockplan#/myAccount/gainsLosses\n\n".to_string());
    }

    // iterate through all sold transactions and update it with needed info
    for (acquisition_date, tr_date, cost_basis, _, inc) in gains_and_losses {
        // match trade date and gross with principal and trade date of  trade confirmation

        log::info!("Reconstructing G&L sold transaction: trade date: {tr_date}, acquisition date: {acquisition_date}, cost basis: {cost_basis}, income: {inc}");
        let trade_date = chrono::NaiveDate::parse_from_str(&tr_date, "%m/%d/%Y")
            .expect_and_log(&format!("Unable to parse trade date: {tr_date}"));

        let (_, settlement_date, _, _, _) = sold_transactions.iter().find(|(trade_dt, _, _, _, income)|{
            log::info!("Candidate Sold transaction from PDF: trade_date: {trade_dt} income: {income}");
            let trade_date_pdf = chrono::NaiveDate::parse_from_str(&trade_dt, "%m/%d/%y").expect_and_log(&format!("Unable to parse trade date: {trade_dt}"));
            trade_date ==  trade_date_pdf
        }).ok_or(format!("\n\nERROR: Sold transaction in Gain&Losses:\n (trade_date: {tr_date}, acquisition date: {acquisition_date}, cost basis: {cost_basis}, income: {inc}) exist,\n but corressponding data from PDF document is missing. You can download account statements PDF documents at:\n
            https://edoc.etrade.com/e/t/onlinedocs/docsearch?doc_type=stmt\n\n"))?;

        detailed_sold_transactions.push((
            chrono::NaiveDate::parse_from_str(&tr_date, "%m/%d/%Y")
                .expect(&format!("Unable to parse trade date: {tr_date}"))
                .format("%m/%d/%y")
                .to_string(),
            settlement_date.clone(),
            chrono::NaiveDate::parse_from_str(&acquisition_date, "%m/%d/%Y")
                .expect(&format!(
                    "Unable to parse acquisition_date: {acquisition_date}"
                ))
                .format("%m/%d/%y")
                .to_string(),
            *inc,
            *cost_basis,
        ));
    }

    Ok(detailed_sold_transactions)
}

pub fn create_detailed_revolut_transactions(
    transactions: Vec<(String, crate::Currency, crate::Currency)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, f32)>>,
) -> Result<Vec<Transaction>, &str> {
    let mut detailed_transactions: Vec<Transaction> = Vec::new();

    transactions
        .iter()
        .try_for_each(|(transaction_date, gross, tax)| {
            let (exchange_rate_date, exchange_rate) = dates
                [&gross.derive_exchange(transaction_date.clone())]
                .clone()
                .unwrap();

            let transaction = Transaction {
                transaction_date: transaction_date.clone(),
                gross: *gross,
                tax_paid: *tax,
                exchange_rate_date,
                exchange_rate,
            };

            let msg = transaction.format_to_print("REVOLUT")?;

            println!("{}", msg);
            log::info!("{}", msg);
            detailed_transactions.push(transaction);
            Ok::<(), &str>(())
        })?;
    Ok(detailed_transactions)
}

pub fn create_detailed_interests_transactions(
    transactions: Vec<(String, f32)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, f32)>>,
) -> Result<Vec<Transaction>, &str> {
    let mut detailed_transactions: Vec<Transaction> = Vec::new();
    transactions
        .iter()
        .try_for_each(|(transaction_date, gross_us)| {
            let (exchange_rate_date, exchange_rate) = dates
                [&crate::Exchange::USD(transaction_date.clone())]
                .clone()
                .unwrap();

            let transaction = Transaction {
                transaction_date: transaction_date.clone(),
                gross: crate::Currency::USD(*gross_us as f64),
                tax_paid: crate::Currency::USD(0.0 as f64),
                exchange_rate_date,
                exchange_rate,
            };

            let msg = transaction.format_to_print("INTERESTS")?;

            println!("{}", msg);
            log::info!("{}", msg);
            detailed_transactions.push(transaction);
            Ok::<(), &str>(())
        })?;
    Ok(detailed_transactions)
}

pub fn create_detailed_div_transactions(
    transactions: Vec<(String, f32, f32)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, f32)>>,
) -> Result<Vec<Transaction>, &str> {
    let mut detailed_transactions: Vec<Transaction> = Vec::new();
    transactions
        .iter()
        .try_for_each(|(transaction_date, gross_us, tax_us)| {
            let (exchange_rate_date, exchange_rate) = dates
                [&crate::Exchange::USD(transaction_date.clone())]
                .clone()
                .unwrap();

            let transaction = Transaction {
                transaction_date: transaction_date.clone(),
                gross: crate::Currency::USD(*gross_us as f64),
                tax_paid: crate::Currency::USD(*tax_us as f64),
                exchange_rate_date,
                exchange_rate,
            };

            let msg = transaction.format_to_print("DIV")?;

            println!("{}", msg);
            log::info!("{}", msg);
            detailed_transactions.push(transaction);
            Ok::<(), &str>(())
        })?;
    Ok(detailed_transactions)
}

//    pub trade_date: String,
//    pub settlement_date: String,
//    pub acquisition_date: String,
//    pub income_us: f32,
//    pub cost_basis: f32,
//    pub exchange_rate_settlement_date: String,
//    pub exchange_rate_settlement: f32,
//    pub exchange_rate_acquisition_date: String,
//    pub exchange_rate_acquisition: f32,
pub fn create_detailed_sold_transactions(
    transactions: Vec<(String, String, String, f32, f32)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, f32)>>,
) -> Result<Vec<SoldTransaction>, &str> {
    let mut detailed_transactions: Vec<SoldTransaction> = Vec::new();
    transactions.iter().for_each(
        |(trade_date, settlement_date, acquisition_date, income, cost_basis)| {
            let (exchange_rate_settlement_date, exchange_rate_settlement) = dates
                [&crate::Exchange::USD(settlement_date.clone())]
                .clone()
                .unwrap();
            let (exchange_rate_acquisition_date, exchange_rate_acquisition) = dates
                [&crate::Exchange::USD(acquisition_date.clone())]
                .clone()
                .unwrap();

            let transaction = SoldTransaction {
                settlement_date: settlement_date.clone(),
                trade_date: trade_date.clone(),
                acquisition_date: acquisition_date.clone(),
                income_us: *income,
                cost_basis: *cost_basis,
                exchange_rate_settlement_date,
                exchange_rate_settlement,
                exchange_rate_acquisition_date,
                exchange_rate_acquisition,
            };

            let msg = transaction.format_to_print();

            println!("{}", msg);
            log::info!("{}", msg);

            detailed_transactions.push(transaction);
        },
    );
    Ok(detailed_transactions)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_interests_verification_ok() -> Result<(), String> {
        let transactions: Vec<(String, f32)> = vec![
            ("06/01/21".to_string(), 100.0),
            ("03/01/21".to_string(), 126.0),
        ];
        verify_interests_transactions(&transactions)
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
    fn test_create_detailed_revolut_transactions_eur() -> Result<(), String> {
        let parsed_transactions = vec![
            (
                "03/01/21".to_owned(),
                crate::Currency::EUR(0.05),
                crate::Currency::EUR(0.00),
            ),
            (
                "04/11/21".to_owned(),
                crate::Currency::EUR(0.07),
                crate::Currency::EUR(0.00),
            ),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, f32)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::EUR("03/01/21".to_owned()),
            Some(("02/28/21".to_owned(), 2.0)),
        );
        dates.insert(
            crate::Exchange::EUR("04/11/21".to_owned()),
            Some(("04/10/21".to_owned(), 3.0)),
        );

        let transactions = create_detailed_revolut_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::EUR(0.05),
                    tax_paid: crate::Currency::EUR(0.0),
                    exchange_rate_date: "02/28/21".to_string(),
                    exchange_rate: 2.0,
                },
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::EUR(0.07),
                    tax_paid: crate::Currency::EUR(0.0),
                    exchange_rate_date: "04/10/21".to_string(),
                    exchange_rate: 3.0,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_revolut_transactions_pln() -> Result<(), String> {
        let parsed_transactions = vec![
            (
                "03/01/21".to_owned(),
                crate::Currency::PLN(0.44),
                crate::Currency::PLN(0.00),
            ),
            (
                "04/11/21".to_owned(),
                crate::Currency::PLN(0.45),
                crate::Currency::PLN(0.00),
            ),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, f32)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::PLN("03/01/21".to_owned()),
            Some(("N/A".to_owned(), 1.0)),
        );
        dates.insert(
            crate::Exchange::PLN("04/11/21".to_owned()),
            Some(("N/A".to_owned(), 1.0)),
        );

        let transactions = create_detailed_revolut_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::PLN(0.44),
                    tax_paid: crate::Currency::PLN(0.0),
                    exchange_rate_date: "N/A".to_string(),
                    exchange_rate: 1.0,
                },
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::PLN(0.45),
                    tax_paid: crate::Currency::PLN(0.0),
                    exchange_rate_date: "N/A".to_string(),
                    exchange_rate: 1.0,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_interests_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, f32)> = vec![
            ("04/11/21".to_string(), 100.0),
            ("03/01/21".to_string(), 126.0),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, f32)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::USD("03/01/21".to_owned()),
            Some(("02/28/21".to_owned(), 2.0)),
        );
        dates.insert(
            crate::Exchange::USD("04/11/21".to_owned()),
            Some(("04/10/21".to_owned(), 3.0)),
        );

        let transactions = create_detailed_interests_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::USD(100.0),
                    tax_paid: crate::Currency::USD(0.0),
                    exchange_rate_date: "04/10/21".to_string(),
                    exchange_rate: 3.0,
                },
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::USD(126.0),
                    tax_paid: crate::Currency::USD(0.0),
                    exchange_rate_date: "02/28/21".to_string(),
                    exchange_rate: 2.0,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_div_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, f32, f32)> = vec![
            ("04/11/21".to_string(), 100.0, 25.0),
            ("03/01/21".to_string(), 126.0, 10.0),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, f32)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::USD("03/01/21".to_owned()),
            Some(("02/28/21".to_owned(), 2.0)),
        );
        dates.insert(
            crate::Exchange::USD("04/11/21".to_owned()),
            Some(("04/10/21".to_owned(), 3.0)),
        );

        let transactions = create_detailed_div_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::USD(100.0),
                    tax_paid: crate::Currency::USD(25.0),
                    exchange_rate_date: "04/10/21".to_string(),
                    exchange_rate: 3.0,
                },
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::USD(126.0),
                    tax_paid: crate::Currency::USD(10.0),
                    exchange_rate_date: "02/28/21".to_string(),
                    exchange_rate: 2.0,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_sold_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, String, String, f32, f32)> = vec![
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                "01/01/21".to_string(),
                20.0,
                20.0,
            ),
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                "01/01/19".to_string(),
                25.0,
                10.0,
            ),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, f32)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::USD("01/01/21".to_owned()),
            Some(("12/30/20".to_owned(), 1.0)),
        );
        dates.insert(
            crate::Exchange::USD("03/01/21".to_owned()),
            Some(("02/28/21".to_owned(), 2.0)),
        );
        dates.insert(
            crate::Exchange::USD("03/03/21".to_owned()),
            Some(("03/02/21".to_owned(), 2.5)),
        );
        dates.insert(
            crate::Exchange::USD("06/01/21".to_owned()),
            Some(("06/03/21".to_owned(), 3.0)),
        );
        dates.insert(
            crate::Exchange::USD("06/03/21".to_owned()),
            Some(("06/05/21".to_owned(), 4.0)),
        );
        dates.insert(
            crate::Exchange::USD("01/01/21".to_owned()),
            Some(("02/28/21".to_owned(), 5.0)),
        );
        dates.insert(
            crate::Exchange::USD("01/01/19".to_owned()),
            Some(("12/30/18".to_owned(), 6.0)),
        );
        dates.insert(
            crate::Exchange::USD("04/11/21".to_owned()),
            Some(("04/10/21".to_owned(), 7.0)),
        );

        let transactions = create_detailed_sold_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                SoldTransaction {
                    trade_date: "03/01/21".to_string(),
                    settlement_date: "03/03/21".to_string(),
                    acquisition_date: "01/01/21".to_string(),
                    income_us: 20.0,
                    cost_basis: 20.0,
                    exchange_rate_settlement_date: "03/02/21".to_string(),
                    exchange_rate_settlement: 2.5,
                    exchange_rate_acquisition_date: "02/28/21".to_string(),
                    exchange_rate_acquisition: 5.0,
                },
                SoldTransaction {
                    trade_date: "06/01/21".to_string(),
                    settlement_date: "06/03/21".to_string(),
                    acquisition_date: "01/01/19".to_string(),
                    income_us: 25.0,
                    cost_basis: 10.0,
                    exchange_rate_settlement_date: "06/05/21".to_string(),
                    exchange_rate_settlement: 4.0,
                    exchange_rate_acquisition_date: "12/30/18".to_string(),
                    exchange_rate_acquisition: 6.0,
                },
            ])
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
    fn test_sold_transaction_reconstruction_dividiends_only() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32)> = vec![];

        let parsed_gains_and_losses: Vec<(String, String, f32, f32, f32)> = vec![];

        let detailed_sold_transactions =
            reconstruct_sold_transactions(&parsed_sold_transactions, &parsed_gains_and_losses)?;
        // 1. trade date
        // 2. settlement date
        // 3. date of purchase
        // 4. net income
        // 5. cost cost basis
        assert_eq!(detailed_sold_transactions, vec![]);
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_ok() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32)> = vec![
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                1.0,
                25.0,
                24.8,
            ),
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                2.0,
                10.0,
                19.8,
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, f32, f32, f32)> = vec![
            (
                "01/01/2019".to_string(),
                "06/01/2021".to_string(),
                10.0,
                10.0,
                24.8,
            ),
            (
                "01/01/2021".to_string(),
                "03/01/2021".to_string(),
                20.0,
                20.0,
                19.8,
            ),
        ];

        let detailed_sold_transactions =
            reconstruct_sold_transactions(&parsed_sold_transactions, &parsed_gains_and_losses)?;

        // 1. trade date
        // 2. settlement date
        // 3. date of purchase
        // 4. net income
        // 5. cost cost basis
        assert_eq!(
            detailed_sold_transactions,
            vec![
                (
                    "06/01/21".to_string(),
                    "06/03/21".to_string(),
                    "01/01/19".to_string(),
                    24.8,
                    10.0
                ),
                (
                    "03/01/21".to_string(),
                    "03/03/21".to_string(),
                    "01/01/21".to_string(),
                    19.8,
                    20.0
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_single_digits_ok() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32)> = vec![
            ("6/1/21".to_string(), "6/3/21".to_string(), 1.0, 25.0, 24.8),
            ("3/1/21".to_string(), "3/3/21".to_string(), 2.0, 10.0, 19.8),
        ];

        let parsed_gains_and_losses: Vec<(String, String, f32, f32, f32)> = vec![
            (
                "01/01/2019".to_string(),
                "06/01/2021".to_string(),
                10.0,
                10.0,
                24.8,
            ),
            (
                "01/01/2021".to_string(),
                "03/01/2021".to_string(),
                20.0,
                20.0,
                19.8,
            ),
        ];

        let detailed_sold_transactions =
            reconstruct_sold_transactions(&parsed_sold_transactions, &parsed_gains_and_losses)?;

        // 1. trade date
        // 2. settlement date
        // 3. date of purchase
        // 4. net income
        // 5. cost cost basis
        assert_eq!(
            detailed_sold_transactions,
            vec![
                (
                    "06/01/21".to_string(),
                    "6/3/21".to_string(),
                    "01/01/19".to_string(),
                    24.8,
                    10.0
                ),
                (
                    "03/01/21".to_string(),
                    "3/3/21".to_string(),
                    "01/01/21".to_string(),
                    19.8,
                    20.0
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_second_fail() {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32)> = vec![(
            "11/07/22".to_string(), // trade date
            "11/09/22".to_string(), // settlement date
            173.0,                  // quantity
            28.2035,                // price
            4877.36,                // amount sold
        )];

        let parsed_gains_and_losses: Vec<(String, String, f32, f32, f32)> = vec![
            (
                "05/02/22".to_string(), // date when sold stock was acquired (date_acquired)
                "07/19/22".to_string(), // date when stock was sold (date_sold)
                0.0,                    // aqusition cost of sold stock (aquisition_cost)
                1593.0,                 // adjusted aquisition cost of sold stock (cost_basis)
                1415.480004,            // income from sold stock (total_proceeds)
            ),
            (
                "02/18/22".to_string(),
                "07/19/22".to_string(),
                4241.16,
                4989.6,
                4325.10001,
            ),
            (
                "08/19/22".to_string(),
                "11/07/22".to_string(),
                5236.0872,
                6160.0975,
                4877.355438,
            ),
        ];

        assert_eq!(
            reconstruct_sold_transactions(&parsed_sold_transactions, &parsed_gains_and_losses)
                .is_ok(),
            false
        );
    }

    #[test]
    fn test_sold_transaction_reconstruction_multistock() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32)> = vec![
            (
                "12/21/22".to_string(),
                "12/23/22".to_string(),
                163.0,
                26.5900,
                4332.44,
            ),
            (
                "12/19/22".to_string(),
                "12/21/22".to_string(),
                252.0,
                26.5900,
                6698.00,
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, f32, f32, f32)> = vec![
            (
                "08/19/2021".to_string(),
                "12/19/2022".to_string(),
                4336.4874,
                4758.6971,
                2711.0954,
            ),
            (
                "05/03/2021".to_string(),
                "12/21/2022".to_string(),
                0.0,
                3876.918,
                2046.61285,
            ),
            (
                "08/19/2022".to_string(),
                "12/19/2022".to_string(),
                5045.6257,
                5936.0274,
                3986.9048,
            ),
            (
                "05/02/2022".to_string(),
                "12/21/2022".to_string(),
                0.0,
                4013.65,
                2285.82733,
            ),
        ];

        let detailed_sold_transactions =
            reconstruct_sold_transactions(&parsed_sold_transactions, &parsed_gains_and_losses)?;

        assert_eq!(
            detailed_sold_transactions,
            vec![
                (
                    "12/19/22".to_string(),
                    "12/21/22".to_string(),
                    "08/19/21".to_string(),
                    2711.0954,
                    4336.4874,
                ),
                (
                    "12/21/22".to_string(),
                    "12/23/22".to_string(),
                    "05/03/21".to_string(),
                    2046.61285,
                    0.0,
                ),
                (
                    "12/19/22".to_string(),
                    "12/21/22".to_string(),
                    "08/19/22".to_string(),
                    3986.9048,
                    5045.6257,
                ),
                (
                    "12/21/22".to_string(),
                    "12/23/22".to_string(),
                    "05/02/22".to_string(),
                    2285.82733,
                    0.0,
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_no_gains_fail() {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32)> = vec![
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                1.0,
                25.0,
                24.8,
            ),
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                2.0,
                10.0,
                19.8,
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, f32, f32, f32)> = vec![];

        let result =
            reconstruct_sold_transactions(&parsed_sold_transactions, &parsed_gains_and_losses);
        assert_eq!( result , Err("\n\nERROR: Sold transaction detected, but corressponding Gain&Losses document is missing. Please download Gain&Losses  XLSX document at:\n
            https://us.etrade.com/etx/sp/stockplan#/myAccount/gainsLosses\n\n".to_string()));
    }
}
