// SPDX-FileCopyrightText: 2022-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

use chrono;
use chrono::Datelike;
use polars::prelude::*;
use std::collections::HashMap;

pub use crate::logging::ResultExt;
use crate::{SoldTransaction, Transaction};

/// Check if all interests rate transactions come from the same year
pub fn verify_interests_transactions<T>(transactions: &Vec<(String, T, T)>) -> Result<(), String> {
    let mut trans = transactions.iter();
    let transaction_date = match trans.next() {
        Some((x, _, _)) => x,
        None => {
            log::info!("No interests transactions");
            return Ok(());
        }
    };

    let transaction_year = chrono::NaiveDate::parse_from_str(transaction_date, "%m/%d/%y")
        .map_err(|_| format!("Unable to parse transaction date: \"{transaction_date}\""))?
        .year();
    let mut verification: Result<(), String> = Ok(());
    trans.try_for_each(|(tr_date, _, _)| {
        let tr_year = chrono::NaiveDate::parse_from_str(tr_date, "%m/%d/%y")
            .map_err(|_| format!("Unable to parse transaction date: \"{tr_date}\""))?
            .year();
        if tr_year != transaction_year {
            let msg: &str = "Error:  Statements are related to different years!";
            verification = Err(msg.to_owned());
        }
        Ok::<(), String>(())
    })?;
    verification
}

/// Check if all dividends transaction come from the same year
pub fn verify_dividends_transactions<T>(
    div_transactions: &Vec<(String, T, T, Option<String>)>,
) -> Result<(), String> {
    let mut trans = div_transactions.iter();
    let transaction_date = match trans.next() {
        Some((x, _, _, _)) => x,
        None => {
            log::info!("No Dividends transactions");
            return Ok(());
        }
    };

    let transaction_year = chrono::NaiveDate::parse_from_str(transaction_date, "%m/%d/%y")
        .map_err(|_| format!("Unable to parse transaction date: \"{transaction_date}\""))?
        .year();
    let mut verification: Result<(), String> = Ok(());
    trans.try_for_each(|(tr_date, _, _, _)| {
        let tr_year = chrono::NaiveDate::parse_from_str(tr_date, "%m/%d/%y")
            .map_err(|_| format!("Unable to parse transaction date: \"{tr_date}\""))?
            .year();
        if tr_year != transaction_year {
            let msg: &str = "Error:  Statements are related to different years!";
            verification = Err(msg.to_owned());
        }
        Ok::<(), String>(())
    })?;
    verification
}

pub fn verify_transactions<T>(transactions: &Vec<(String, String, T, T, Option<String>)>) -> Result<(), String> {
    let mut trans = transactions.iter();
    let transaction_date = match trans.next() {
        Some((_, x, _, _, _)) => x,
        None => {
            log::info!("No revolut sold transactions");
            return Ok(());
        }
    };

    let transaction_year = chrono::NaiveDate::parse_from_str(transaction_date, "%m/%d/%y")
        .map_err(|_| format!("Unable to parse transaction date: \"{transaction_date}\""))?
        .year();
    let mut verification: Result<(), String> = Ok(());
    trans.try_for_each(|(_, tr_date, _, _, _)| {
        let tr_year = chrono::NaiveDate::parse_from_str(tr_date, "%m/%d/%y")
            .map_err(|_| format!("Unable to parse transaction date: \"{tr_date}\""))?
            .year();
        if tr_year != transaction_year {
            let msg: &str = "Error: Statements are related to different years!";
            verification = Err(msg.to_owned());
        }
        Ok::<(), String>(())
    })?;
    verification
}

/// Trade date is when transaction was trigerred.
/// fees and commission are applied at the moment of settlement date so
/// we ignore those and use net income rather than principal
/// Actual Tax is to be paid from settlement_date
pub fn reconstruct_sold_transactions(
    sold_transactions: &Vec<(String, String, f32, f32, f32, Option<String>)>,
    gains_and_losses: &Vec<(String, String, f32, f32, f32)>,
) -> Result<Vec<(String, String, String, f32, f32, Option<String>)>, String> {
    // Ok What do I need.
    // 1. trade date
    // 2. settlement date
    // 3. date of purchase
    // 4. gross income
    // 5. cost cost basis
    // 6. company symbol (ticker)
    let mut detailed_sold_transactions: Vec<(String, String, String, f32, f32, Option<String>)> = vec![];

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

        let (_, settlement_date, _, _, _, symbol) = sold_transactions.iter().find(|(trade_dt, _, _, _, income, _)|{
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
            symbol.clone(),
        ));
    }

    Ok(detailed_sold_transactions)
}

pub fn create_detailed_revolut_transactions(
    transactions: Vec<(String, crate::Currency, crate::Currency, Option<String>)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, f32)>>,
) -> Result<Vec<Transaction>, &str> {
    let mut detailed_transactions: Vec<Transaction> = Vec::new();

    transactions
        .iter()
        .try_for_each(|(transaction_date, gross, tax, company)| {
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
                company: company.clone(),
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
                company: None, // No company info when interests are paid on money
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
    transactions: Vec<(String, f32, f32, Option<String>)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, f32)>>,
) -> Result<Vec<Transaction>, &str> {
    let mut detailed_transactions: Vec<Transaction> = Vec::new();
    transactions
        .iter()
        .try_for_each(|(transaction_date, gross_us, tax_us, company)| {
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
                company: company.clone(),
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
    transactions: Vec<(String, String, String, f32, f32, Option<String>)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, f32)>>,
) -> Result<Vec<SoldTransaction>, &str> {
    let mut detailed_transactions: Vec<SoldTransaction> = Vec::new();
    transactions.iter().for_each(
        |(trade_date, settlement_date, acquisition_date, income, cost_basis, symbol)| {
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
                company : symbol.clone(),
            };

            let msg = transaction.format_to_print("");

            println!("{}", msg);
            log::info!("{}", msg);

            detailed_transactions.push(transaction);
        },
    );
    Ok(detailed_transactions)
}

pub fn create_detailed_revolut_sold_transactions(
    transactions: Vec<(String, String, crate::Currency, crate::Currency, Option<String>)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, f32)>>,
) -> Result<Vec<SoldTransaction>, &str> {
    let mut detailed_transactions: Vec<SoldTransaction> = Vec::new();
    transactions
        .iter()
        .for_each(|(acquired_date, sold_date, cost_basis, gross_income, symbol)| {
            let (exchange_rate_settlement_date, exchange_rate_settlement) = dates
                [&gross_income.derive_exchange(sold_date.clone())] // TODO: settlement date???
                .clone()
                .unwrap();
            let (exchange_rate_acquisition_date, exchange_rate_acquisition) = dates
                [&cost_basis.derive_exchange(acquired_date.clone())]
                .clone()
                .unwrap();

            let transaction = SoldTransaction {
                settlement_date: sold_date.clone(),
                trade_date: sold_date.clone(),
                acquisition_date: acquired_date.clone(),
                income_us: (gross_income.value() as f32),
                cost_basis: (cost_basis.value() as f32),
                exchange_rate_settlement_date,
                exchange_rate_settlement,
                exchange_rate_acquisition_date,
                exchange_rate_acquisition,
                company : symbol.clone(),
            };

            let msg = transaction.format_to_print("REVOLUT ");

            println!("{}", msg);
            log::info!("{}", msg);

            detailed_transactions.push(transaction);
        });
    Ok(detailed_transactions)
}

// Make a dataframe with
pub(crate) fn create_per_company_report(
    interests: &[Transaction],
    dividends: &[Transaction],
    sold_transactions: &[SoldTransaction],
    revolut_dividends_transactions: &[Transaction],
    revolut_sold_transactions: &[SoldTransaction],
) -> Result<DataFrame, &'static str> {
    // Key: Company Name , Value : (gross_pl, tax_paid_in_us_pl, cost_pl)
    let mut per_company_data: HashMap<Option<String>, (f32, f32, f32)> = HashMap::new();

    let interests_or_dividends = interests
        .iter()
        .chain(dividends.iter())
        .chain(revolut_dividends_transactions.iter());

    interests_or_dividends.for_each(|x| {
        let entry = per_company_data
            .entry(x.company.clone())
            .or_insert((0.0, 0.0, 0.0));
        entry.0 += x.exchange_rate * x.gross.value() as f32;
        entry.1 += x.exchange_rate * x.tax_paid.value() as f32;
        // No cost for dividends being paid
    });

    let sells = sold_transactions
        .iter()
        .chain(revolut_sold_transactions.iter());
    sells.for_each(|x| {
        let entry = per_company_data.entry(None).or_insert((0.0, 0.0, 0.0));
        entry.0 += x.income_us * x.exchange_rate_settlement;
        // No tax from sold transactions
        entry.2 += x.cost_basis * x.exchange_rate_acquisition;
    });

    // Convert my HashMap into DataFrame
    let mut companies: Vec<Option<String>> = Vec::new();
    let mut gross: Vec<f32> = Vec::new();
    let mut tax: Vec<f32> = Vec::new();
    let mut cost: Vec<f32> = Vec::new();
    per_company_data
        .iter()
        .try_for_each(|(company, (gross_pl, tax_paid_in_us_pl, cost_pl))| {
            //log::info!(
            println!(
                "Company: {:?}, Gross PLN: {:.2}, Tax Paid in USD PLN: {:.2}, Cost PLN: {:.2}",
                company, gross_pl, tax_paid_in_us_pl, cost_pl
            );
            companies.push(company.clone());
            gross.push(*gross_pl);
            tax.push(*tax_paid_in_us_pl);
            cost.push(*cost_pl);

            Ok::<(), &str>(())
        })?;
    let series = vec![
        Series::new("Company", companies),
        Series::new("Gross[PLN]", gross),
        Series::new("Cost[PLN]", cost),
        Series::new("Tax Paid in USD[PLN]", tax),
    ];
    DataFrame::new(series).map_err(|_| "Unable to create per company report dataframe")
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::Currency;

    fn round4(val: f64) -> f64 {
        (val * 10_000.0).round() / 10_000.0
    }

    #[test]
    fn test_create_per_company_report_interests() -> Result<(), String> {
        let input = vec![
            Transaction {
                transaction_date: "03/01/21".to_string(),
                gross: crate::Currency::EUR(0.05),
                tax_paid: crate::Currency::EUR(0.0),
                exchange_rate_date: "02/28/21".to_string(),
                exchange_rate: 2.0,
                company: None,
            },
            Transaction {
                transaction_date: "04/11/21".to_string(),
                gross: crate::Currency::EUR(0.07),
                tax_paid: crate::Currency::EUR(0.0),
                exchange_rate_date: "04/10/21".to_string(),
                exchange_rate: 3.0,
                company: None,
            },
        ];
        let df = create_per_company_report(&input, &[], &[], &[], &[])
            .map_err(|e| format!("Error creating per company report: {}", e))?;

        // Interests are having company == None, and data should be folded to one row
        assert_eq!(df.height(), 1);
        assert_eq!(df.width(), 4);

        let company_col = df.column("Company").unwrap();
        assert_eq!(company_col.get(0).is_err(), false); // None company
        let gross_col = df.column("Gross[PLN]").unwrap();
        assert_eq!(
            round4(gross_col.get(0).unwrap().extract::<f64>().unwrap()),
            round4(0.05 * 2.0 + 0.07 * 3.0)
        );
        let cost_col = df.column("Cost[PLN]").unwrap();
        assert_eq!(cost_col.get(0).unwrap().extract::<f64>().unwrap(), 0.00);
        let tax_col = df.column("Tax Paid in USD[PLN]").unwrap();
        assert_eq!(tax_col.get(0).unwrap().extract::<f64>().unwrap(), 0.00);

        Ok(())
    }

    #[test]
    fn test_create_per_company_report_dividends() -> Result<(), String> {
        let input = vec![
            Transaction {
                transaction_date: "04/11/21".to_string(),
                gross: crate::Currency::USD(100.0),
                tax_paid: crate::Currency::USD(25.0),
                exchange_rate_date: "04/10/21".to_string(),
                exchange_rate: 3.0,
                company: Some("INTEL CORP".to_owned()),
            },
            Transaction {
                transaction_date: "03/01/21".to_string(),
                gross: crate::Currency::USD(126.0),
                tax_paid: crate::Currency::USD(10.0),
                exchange_rate_date: "02/28/21".to_string(),
                exchange_rate: 2.0,
                company: Some("INTEL CORP".to_owned()),
            },
            Transaction {
                transaction_date: "03/11/21".to_string(),
                gross: crate::Currency::USD(100.0),
                tax_paid: crate::Currency::USD(0.0),
                exchange_rate_date: "02/28/21".to_string(),
                exchange_rate: 10.0,
                company: Some("ABEV".to_owned()),
            },
        ];
        let df = create_per_company_report(&[], &input, &[], &[], &[])
            .map_err(|e| format!("Error creating per company report: {}", e))?;

        // Interests are having company == None, and data should be folded to one row
        assert_eq!(df.height(), 2);
        assert_eq!(df.width(), 4);

        let company_col = df.column("Company").unwrap().utf8().unwrap();
        let gross_col = df.column("Gross[PLN]").unwrap();
        let tax_col = df.column("Tax Paid in USD[PLN]").unwrap();
        let (abev_index, intc_index) = match company_col.get(0) {
            Some("INTEL CORP") => (1, 0),
            Some("ABEV") => (0, 1),
            _ => return Err("Unexpected company name in first row".to_owned()),
        };
        assert_eq!(
            round4(gross_col.get(intc_index).unwrap().extract::<f64>().unwrap()),
            round4(100.0 * 3.0 + 126.0 * 2.0)
        );
        assert_eq!(
            round4(gross_col.get(abev_index).unwrap().extract::<f64>().unwrap()),
            round4(100.0 * 10.0)
        );
        assert_eq!(
            tax_col.get(intc_index).unwrap().extract::<f64>().unwrap(),
            round4(25.0 * 3.0 + 10.0 * 2.0)
        );
        assert_eq!(
            tax_col.get(abev_index).unwrap().extract::<f64>().unwrap(),
            round4(0.0)
        );

        let cost_col = df.column("Cost[PLN]").unwrap();
        assert_eq!(cost_col.get(0).unwrap().extract::<f64>().unwrap(), 0.00);
        assert_eq!(cost_col.get(1).unwrap().extract::<f64>().unwrap(), 0.00);

        Ok(())
    }

    #[test]
    fn test_interests_verification_ok() -> Result<(), String> {
        let transactions: Vec<(String, f32, f32)> = vec![
            ("06/01/21".to_string(), 100.0, 0.00),
            ("03/01/21".to_string(), 126.0, 0.00),
        ];
        verify_interests_transactions(&transactions)
    }

    #[test]
    fn test_revolut_sold_verification_false() -> Result<(), String> {
        let transactions: Vec<(String, String, Currency, Currency, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                "06/01/22".to_string(),
                Currency::PLN(10.0),
                Currency::PLN(2.0),
                Some("INTEL CORP".to_owned()),
            ),
            (
                "06/01/21".to_string(),
                "07/04/23".to_string(),
                Currency::PLN(10.0),
                Currency::PLN(2.0),
                Some("INTEL CORP".to_owned()),
            ),
        ];
        assert_eq!(
            verify_transactions(&transactions),
            Err("Error: Statements are related to different years!".to_owned())
        );
        Ok(())
    }

    #[test]
    fn test_dividends_verification_ok() -> Result<(), String> {
        let transactions: Vec<(String, f32, f32, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                100.0,
                25.0,
                Some("INTEL CORP".to_owned()),
            ),
            (
                "03/01/21".to_string(),
                126.0,
                10.0,
                Some("INTEL CORP".to_owned()),
            ),
        ];
        verify_dividends_transactions(&transactions)
    }

    #[test]
    fn test_dividends_verification_false() -> Result<(), String> {
        let transactions: Vec<(String, Currency, Currency, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                Currency::PLN(10.0),
                Currency::PLN(2.0),
                Some("INTEL CORP".to_owned()),
            ),
            (
                "03/01/22".to_string(),
                Currency::PLN(126.0),
                Currency::PLN(10.0),
                Some("INTEL CORP".to_owned()),
            ),
        ];
        assert_eq!(
            verify_dividends_transactions(&transactions),
            Err("Error:  Statements are related to different years!".to_owned())
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_revolut_transactions_eur() -> Result<(), String> {
        let parsed_transactions = vec![
            (
                "03/01/21".to_owned(),
                crate::Currency::EUR(0.05),
                crate::Currency::EUR(0.00),
                None,
            ),
            (
                "04/11/21".to_owned(),
                crate::Currency::EUR(0.07),
                crate::Currency::EUR(0.00),
                None,
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
                    company: None,
                },
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::EUR(0.07),
                    tax_paid: crate::Currency::EUR(0.0),
                    exchange_rate_date: "04/10/21".to_string(),
                    exchange_rate: 3.0,
                    company: None,
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
                None,
            ),
            (
                "04/11/21".to_owned(),
                crate::Currency::PLN(0.45),
                crate::Currency::PLN(0.00),
                None,
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
                    company: None,
                },
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::PLN(0.45),
                    tax_paid: crate::Currency::PLN(0.0),
                    exchange_rate_date: "N/A".to_string(),
                    exchange_rate: 1.0,
                    company: None,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_interests_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, f32, f32)> = vec![
            ("04/11/21".to_string(), 100.0, 0.00),
            ("03/01/21".to_string(), 126.0, 0.00),
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
                    company: None,
                },
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::USD(126.0),
                    tax_paid: crate::Currency::USD(0.0),
                    exchange_rate_date: "02/28/21".to_string(),
                    exchange_rate: 2.0,
                    company: None,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_div_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, f32, f32, Option<String>)> = vec![
            (
                "04/11/21".to_string(),
                100.0,
                25.0,
                Some("INTEL CORP".to_owned()),
            ),
            (
                "03/01/21".to_string(),
                126.0,
                10.0,
                Some("INTEL CORP".to_owned()),
            ),
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
                    company: Some("INTEL CORP".to_owned())
                },
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::USD(126.0),
                    tax_paid: crate::Currency::USD(10.0),
                    exchange_rate_date: "02/28/21".to_string(),
                    exchange_rate: 2.0,
                    company: Some("INTEL CORP".to_owned())
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_revolut_sold_transactions() -> Result<(), String> {
                
        let parsed_transactions: Vec<(String, String, Currency, Currency,Option<String> )> = vec![(
            "11/20/23".to_string(),
            "12/08/24".to_string(),
            Currency::USD(5000.0),
            Currency::USD(5804.62),
            Some("INTEL CORP".to_owned()),
        )];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, f32)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::USD("11/20/23".to_owned()),
            Some(("11/19/23".to_owned(), 2.0)),
        );
        dates.insert(
            crate::Exchange::USD("12/08/24".to_owned()),
            Some(("12/06/24".to_owned(), 3.0)),
        );

        let transactions = create_detailed_revolut_sold_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![SoldTransaction {
                trade_date: "12/08/24".to_string(),
                settlement_date: "12/08/24".to_string(),
                acquisition_date: "11/20/23".to_string(),
                income_us: 5804.62,
                cost_basis: 5000.0,
                exchange_rate_settlement_date: "12/06/24".to_string(),
                exchange_rate_settlement: 3.0,
                exchange_rate_acquisition_date: "11/19/23".to_string(),
                exchange_rate_acquisition: 2.0,
                company : Some("INTEL CORP".to_owned()),
            },])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_sold_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, String, String, f32, f32, Option<String>)> = vec![
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                "01/01/21".to_string(),
                20.0,
                20.0,
                Some("INTEL CORP".to_owned()),
            ),
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                "01/01/19".to_string(),
                25.0,
                10.0,
                Some("INTEL CORP".to_owned()),
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
                    company : Some("INTEL CORP".to_owned()),
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
                    company : Some("INTEL CORP".to_owned()),
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_dividends_verification_empty_ok() -> Result<(), String> {
        let transactions: Vec<(String, f32, f32, Option<String>)> = vec![];
        verify_dividends_transactions(&transactions)
    }

    #[test]
    fn test_dividends_verification_fail() -> Result<(), String> {
        let transactions: Vec<(String, f32, f32, Option<String>)> = vec![
            (
                "04/11/22".to_string(),
                100.0,
                25.0,
                Some("INTEL CORP".to_owned()),
            ),
            (
                "03/01/21".to_string(),
                126.0,
                10.0,
                Some("INTEL CORP".to_owned()),
            ),
        ];
        assert!(verify_dividends_transactions(&transactions).is_err());
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_dividiends_only() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32, Option<String>)> = vec![];

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
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                1.0,
                25.0,
                24.8,
                Some("INTEL CORP".to_owned())
            ),
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                2.0,
                10.0,
                19.8,
                Some("INTEL CORP".to_owned())
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
                    10.0,
                    Some("INTEL CORP".to_owned())
                ),
                (
                    "03/01/21".to_string(),
                    "03/03/21".to_string(),
                    "01/01/21".to_string(),
                    19.8,
                    20.0,
                    Some("INTEL CORP".to_owned())
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_single_digits_ok() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32, Option<String>)> = vec![
            ("6/1/21".to_string(), "6/3/21".to_string(), 1.0, 25.0, 24.8, Some("INTEL CORP".to_owned())),
            ("3/1/21".to_string(), "3/3/21".to_string(), 2.0, 10.0, 19.8, Some("INTEL CORP".to_owned())),
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
                    10.0,
                    Some("INTEL CORP".to_owned())
                ),
                (
                    "03/01/21".to_string(),
                    "3/3/21".to_string(),
                    "01/01/21".to_string(),
                    19.8,
                    20.0,
                    Some("INTEL CORP".to_owned())
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_second_fail() {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32, Option<String>)> = vec![(
            "11/07/22".to_string(), // trade date
            "11/09/22".to_string(), // settlement date
            173.0,                  // quantity
            28.2035,                // price
            4877.36,                // amount sold
            Some("INTEL CORP".to_owned()) // company symbol (ticker)
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
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32, Option<String>)> = vec![
            (
                "12/21/22".to_string(),
                "12/23/22".to_string(),
                163.0,
                26.5900,
                4332.44,
                Some("INTEL CORP".to_owned())
            ),
            (
                "12/19/22".to_string(),
                "12/21/22".to_string(),
                252.0,
                26.5900,
                6698.00,
                Some("INTEL CORP".to_owned())
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
                    Some("INTEL CORP".to_owned())
                ),
                (
                    "12/21/22".to_string(),
                    "12/23/22".to_string(),
                    "05/03/21".to_string(),
                    2046.61285,
                    0.0,
                    Some("INTEL CORP".to_owned())
                ),
                (
                    "12/19/22".to_string(),
                    "12/21/22".to_string(),
                    "08/19/22".to_string(),
                    3986.9048,
                    5045.6257,
                    Some("INTEL CORP".to_owned())
                ),
                (
                    "12/21/22".to_string(),
                    "12/23/22".to_string(),
                    "05/02/22".to_string(),
                    2285.82733,
                    0.0,
                    Some("INTEL CORP".to_owned())
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_no_gains_fail() {
        let parsed_sold_transactions: Vec<(String, String, f32, f32, f32, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                1.0,
                25.0,
                24.8,
                Some("INTEL CORP".to_owned())
            ),
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                2.0,
                10.0,
                19.8,
                Some("INTEL CORP".to_owned())
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, f32, f32, f32)> = vec![];

        let result =
            reconstruct_sold_transactions(&parsed_sold_transactions, &parsed_gains_and_losses);
        assert_eq!( result , Err("\n\nERROR: Sold transaction detected, but corressponding Gain&Losses document is missing. Please download Gain&Losses  XLSX document at:\n
            https://us.etrade.com/etx/sp/stockplan#/myAccount/gainsLosses\n\n".to_string()));
    }
}
