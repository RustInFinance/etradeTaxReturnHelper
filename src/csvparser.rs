// SPDX-FileCopyrightText: 2023-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

use nom::{
    bytes::complete::tag, bytes::complete::take, error::Error, multi::many_m_n,
    number::complete::double, sequence::tuple,
};
use polars::prelude::*;

enum ParsingState {
    None,
    Crypto(String),
    InterestsEUR(String),
    InterestsPLN(String),
    SellEUR(String),
    SellUSD(String),
    DividendsEUR(String),
    DividendsUSD(String),
}

#[derive(Default)]
struct InvestmentTransactions {
    pub acquired_dates: Vec<String>,
    pub sold_dates: Vec<String>,
    pub costs: Vec<crate::Currency>,
    pub gross: Vec<crate::Currency>,
    pub symbols: Vec<Option<String>>,
}
#[derive(Default)]
struct TransactionAccumulator {
    pub stock: InvestmentTransactions,
    pub crypto: InvestmentTransactions,
    pub dates: Vec<String>,
    pub incomes: Vec<crate::Currency>,
    pub taxes: Vec<crate::Currency>,
    pub symbols: Vec<Option<String>>,
}

#[derive(Debug, PartialEq)]
pub struct RevolutTransactions {
    pub dividend_transactions: Vec<(String, crate::Currency, crate::Currency, Option<String>)>,
    pub sold_transactions: Vec<(
        String,
        String,
        crate::Currency,
        crate::Currency,
        Option<String>,
    )>,
    pub crypto_transactions: Vec<(String, String, crate::Currency, crate::Currency)>,
}

fn extract_cash_with_currency(cashline: &str, currency: &str) -> Result<crate::Currency, String> {
    log::info!("Entry cacheline: {cashline}");
    log::info!("Entry currency: {currency}");

    let cashline_string: String = cashline.to_string().replace(",", "");
    let mut pln_parser = tuple((double::<&str, Error<_>>, take(1usize), tag("PLN")));
    let mut usd_parser = tuple((tag("$"), double::<&str, Error<_>>));

    // Let's check if We can convert value of currency to f64 directly
    let value: f64 = cashline_string
        .parse::<f64>()
        .map_err(|_| format!("error parsing \"{cashline_string}\" to f64"))
        .or_else(|_| {
            let (_, (value, _, _)) = pln_parser(cashline_string.as_str())
                .map_err(|_| format!("error converting string: \"{cashline_string}\" to f64"))?;
            Ok::<f64, String>(value)
        })
        .or_else(|_| {
            let (_, (_, value)) = usd_parser(cashline_string.as_str())
                .map_err(|_| format!("error converting string: \"{cashline_string}\" to f64"))?;
            Ok::<f64, String>(value)
        })?;

    match currency {
        "PLN" => Ok(crate::Currency::PLN(value)),
        "USD" => Ok(crate::Currency::USD(value)),
        "EUR" => Ok(crate::Currency::EUR(value)),
        _ => Err(format!("Error converting: {cashline_string}")),
    }
}

fn extract_cash(cashline: &str) -> Result<crate::Currency, String> {
    // We need to erase "," before processing it by parser
    log::info!("Entry moneyin/total amount line: {cashline}");
    // replace "," to "." only if there are is no "." already
    // otherwise remove ','
    let cashline_string: String = if cashline.contains(',') && cashline.contains(".") {
        cashline.to_string().replace(",", "")
    } else {
        cashline.to_string().replace(",", ".")
    };
    let cashline_string: String = cashline_string.replace(" ", "");
    log::info!("Processed moneyin/total amount line: {cashline_string}");
    let mut euro_parser = tuple((double::<&str, Error<_>>, tag("€")));
    let mut euro_parser2 = tuple((tag("€"), double::<&str, Error<_>>));
    let mut usd_parser = tuple((many_m_n(0, 1, tag("-")), tag("$"), double::<&str, Error<_>>));
    let mut usd_parser2 = tuple((many_m_n(0, 1, tag("-")), double::<&str, Error<_>>, tag("$")));
    let mut pln_parser = tuple((double::<&str, Error<_>>, tag("PLN")));

    if let Ok((_, (value, _))) = euro_parser(cashline_string.as_str()) {
        return Ok(crate::Currency::EUR(value));
    } else if let Ok((_, (_, value))) = euro_parser2(cashline_string.as_str()) {
        return Ok(crate::Currency::EUR(value));
    } else if let Ok((_, (value, _))) = pln_parser(cashline_string.as_str()) {
        return Ok(crate::Currency::PLN(value));
    } else if let Ok((_, (sign, _, value))) = usd_parser(cashline_string.as_str()) {
        return Ok(crate::Currency::USD(if sign.len() == 1 {
            -value
        } else {
            value
        }));
    } else if let Ok((_, (sign, value, _))) = usd_parser2(cashline_string.as_str()) {
        return Ok(crate::Currency::USD(if sign.len() == 1 {
            -value
        } else {
            value
        }));
    } else {
        return Err(format!("Error converting: {cashline_string}"));
    }
}

fn extract_dividends_transactions(df: &DataFrame) -> Result<DataFrame, &'static str> {
    let df_transactions = if df.get_column_names().contains(&"Currency") {
        df.select([
            "Date",
            "Symbol",
            "Gross amount",
            "Withholding tax",
            "Currency",
        ])
    } else {
        df.select([
            "Date",
            "Symbol",
            "Gross amount base currency",
            "Net amount base currency",
        ])
    }
    .map_err(|_| "Error: Unable to select collumns in Revolut dividends transactions")?;

    Ok(df_transactions)
}

fn extract_sold_transactions(df: &DataFrame) -> Result<DataFrame, &'static str> {
    let df_transactions = if df.get_column_names().contains(&"Currency") {
        df.select([
            "Date acquired",
            "Date sold",
            "Symbol",
            "Cost basis",
            "Gross proceeds",
            "Currency",
        ])
    } else {
        df.select([
            "Date acquired",
            "Date sold",
            "Symbol",
            "Cost basis base currency",
            "Gross proceeds base currency",
            "Fees  base currency",
        ])
    }
    .map_err(|_| "Error: Unable to select collumns in Revolut sold transactions")?;

    Ok(df_transactions)
}

fn extract_investment_gains_and_costs_transactions(
    df: &DataFrame,
) -> Result<DataFrame, &'static str> {
    let df_transactions = df
        .select(["Date", "Ticker", "Type", "Total Amount"])
        .map_err(|_| "Error: Unable to select description")?;

    let intrest_rate_mask = df_transactions
        .column("Type")
        .map_err(|_| "Error: Unable to get Type")?
        .equal("DIVIDEND")
        .expect("Error creating mask")
        | df_transactions
            .column("Type")
            .map_err(|_| "Error: Unable to get Type")?
            .equal("CUSTODY FEE")
            .expect("Error creating mask");

    let filtred_df = df.filter(&intrest_rate_mask).expect("Error filtering");

    Ok(filtred_df)
}

fn extract_intrest_rate_transactions(df: &DataFrame) -> Result<DataFrame, &'static str> {
    // 1. Get rows with transactions
    let mut df_transactions = if df.get_column_names().contains(&"Completed Date") {
        df.select(&["Description", "Money in", "Completed Date"])
    } else {
        df.select(&["Description", "Money in", "Date"])
    }
    .map_err(|_| "Error: Unable to select collumns in Revolut Interests rate transactions")?;

    // This code maps diffrent Description types related to interests into "odsetki"
    let intrest_rate = df_transactions
        .column("Description")
        .map_err(|_| "Error: Unable to get Description")?
        .iter()
        .map(|x| {
            let m = match x {
                AnyValue::Utf8(x) => {
                    if x.contains("Odsetki brutto")
                        || x.contains("Gross interest")
                        || x.contains("Interest earned")
                    {
                        Some("odsetki")
                    } else {
                        None
                    }
                }
                _ => None,
            };
            m
        })
        .collect::<Vec<_>>();

    // cols: "Completed Date", "Description" , "Money In"
    let new_desc = Series::new("Description", intrest_rate);
    df_transactions
        .with_column(new_desc)
        .expect("Unable to replace Description column");
    let intrest_rate_mask = df_transactions
        .column("Description")
        .map_err(|_| "Error: Unable to get Description")?
        .equal("odsetki")
        .expect("Error creating mask");

    let filtred_df = df.filter(&intrest_rate_mask).expect("Error filtering");
    // I need to get (Currecy, Transaction Data and amount)

    Ok(filtred_df)
}

fn parse_symbols(df: &DataFrame, col_name: &str) -> Result<Vec<Option<String>>, &'static str> {
    let symbol = df
        .column(col_name)
        .map_err(|_| "Error: Unable to select Symbol")?;
    let mut symbols: Vec<Option<String>> = vec![];
    let possible_symbols = symbol
        .utf8()
        .map_err(|_| "Error: Unable to convert to utf8")?;

    possible_symbols.into_iter().try_for_each(|maybe_symbol| {
        if let Some(s) = maybe_symbol {
            symbols.push(Some(s.to_string()));
        } else {
            symbols.push(None);
        }
        Ok::<(), &str>(())
    })?;

    Ok(symbols)
}

fn parse_investment_transaction_dates(
    df: &DataFrame,
    col_name: &str,
) -> Result<Vec<String>, &'static str> {
    let date = df
        .column(col_name)
        .map_err(|_| "Error: Unable to select Date")?;
    let mut dates: Vec<String> = vec![];
    let possible_dates = date
        .utf8()
        .map_err(|_| "Error: Unable to convert to utf8")?;
    possible_dates.into_iter().try_for_each(|x| {
        if let Some(d) = x {
            let d = d
                .replace(" sty ", " Jan ")
                .replace(" lut ", " Feb ")
                .replace(" mar ", " Mar ")
                .replace(" kwi ", " Apr ")
                .replace(" maj ", " May ")
                .replace(" cze ", " Jun ")
                .replace(" lip ", " Jul ")
                .replace(" sie ", " Aug ")
                .replace(" wrz ", " Sep ")
                .replace(" Sept ", " Sep ")
                .replace(" paź ", " Oct ")
                .replace(" lis ", " Nov ")
                .replace(" gru ", " Dec ");
            let cd = chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%dT%H:%M:%S%.fZ")
                .or_else(|_| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d"))
                .or_else(|_| chrono::NaiveDate::parse_from_str(&d, "%e %b %Y"))
                .or_else(|_| chrono::NaiveDate::parse_from_str(&d, "%b %d, %Y"))
                .map_err(|_| "Error converting cell to NaiveDate")?
                .format("%m/%d/%y")
                .to_string();
            dates.push(cd);
        }
        Ok::<(), &str>(())
    })?;

    Ok(dates)
}

fn parse_incomes(df: &DataFrame, col: &str) -> Result<Vec<crate::Currency>, String> {
    let moneyin = df
        .column(col)
        .map_err(|_| format!("Error: Unable to select Money In column '{}'", col))?;
    let possible_incomes = moneyin
        .utf8()
        .map_err(|_| format!("Error: Unable to convert column '{}' to utf8", col))?;

    possible_incomes
        .into_iter()
        .filter_map(|x| x)
        .map(|d| extract_cash(&d))
        .collect()
}

fn parse_income_with_currency(
    df: &DataFrame,
    income_col: &str,
    currency_col: &str,
) -> Result<Vec<crate::Currency>, String> {
    let mut incomes: Vec<crate::Currency> = vec![];
    let moneyin = df
        .column(income_col)
        .map_err(|_| "Error: Unable to select Income column")?;
    let currency = df
        .column(currency_col)
        .map_err(|_| "Error: Unable to select Currency column")?;
    let possible_currency = currency
        .utf8()
        .map_err(|e| format!("Unable to convert to utf8. Error: {e}"))?;
    match moneyin.dtype() {
        DataType::Float64 => {
            let possible_incomes = moneyin
                .f64()
                .map_err(|e| format!("Unable to convert to f64. Error: {e}"))?;

            possible_incomes
                .into_iter()
                .zip(possible_currency)
                .try_for_each(|(x, y)| {
                    if let (Some(d), Some(c)) = (x, y) {
                        incomes.push(extract_cash_with_currency(&format!("{d}"), c)?);
                    }
                    Ok::<(), String>(())
                })?;
        }
        DataType::Utf8 => {
            let possible_incomes = moneyin
                .utf8()
                .map_err(|e| format!("Unable to convert to utf8. Error: {e}"))?;

            possible_incomes
                .into_iter()
                .zip(possible_currency)
                .try_for_each(|(x, y)| {
                    if let (Some(d), Some(c)) = (x, y) {
                        incomes.push(extract_cash_with_currency(d, c)?);
                    }
                    Ok::<(), String>(())
                })?;
        }
        _ => return Err("Error: Unable to convert to utf8 or f64".to_string()),
    }

    Ok(incomes)
}

/// Process gathered financial operations from revolut consolidated tax document
fn process_tax_consolidated_data(
    state: &ParsingState,
    delimiter: u8,
    ta: &mut TransactionAccumulator,
) -> Result<(), String> {
    match state {
        ParsingState::None => {}
        ParsingState::InterestsEUR(s) | ParsingState::InterestsPLN(s) => {
            log::trace!("String to parse of Interests: {s}");
            let df = CsvReader::new(std::io::Cursor::new(s.as_bytes()))
                .truncate_ragged_lines(true)
                .with_separator(delimiter)
                .finish()
                .map_err(|e| format!("Error reading CSV (Interests): {e}"))?;
            log::info!("Content of Interests: {df}");
            let filtred_df = extract_intrest_rate_transactions(&df)?;
            ta.dates
                .extend(parse_investment_transaction_dates(&filtred_df, "Date")?);
            let lincomes = parse_incomes(&filtred_df, "Money in")?;
            ta.symbols.extend(std::iter::repeat_n(None, lincomes.len()));
            let ltaxes: Vec<crate::Currency> = lincomes.iter().map(|i| i.derive(0.0)).collect();
            ta.taxes.extend(ltaxes);
            ta.incomes.extend(lincomes);
        }
        ParsingState::SellEUR(s) | ParsingState::SellUSD(s) => {
            log::trace!("String to parse of Sells: {s}");
            let df = CsvReader::new(std::io::Cursor::new(s.as_bytes()))
                .truncate_ragged_lines(true)
                .with_separator(delimiter)
                .finish()
                .map_err(|e| format!("Error reading CSV (Sells): {e}"))?;
            log::trace!("Content of Sells: {df}");
            let filtred_df = extract_sold_transactions(&df)?;
            log::info!("Filtered Sold Data of interest: {filtred_df}");
            let lacquired_dates = parse_investment_transaction_dates(&filtred_df, "Date acquired")?;
            log::info!("dates:: {:?}", ta.stock.acquired_dates);
            let lsold_dates = parse_investment_transaction_dates(&filtred_df, "Date sold")?;

            // For each sold data has to be one acquire date
            if lacquired_dates.len() != lsold_dates.len() {
                return Err("ERROR: Different number of acquired and sold dates".to_string());
            }
            ta.stock.sold_dates.extend(lsold_dates);
            ta.stock.acquired_dates.extend(lacquired_dates);
            ta.stock
                .symbols
                .extend(parse_symbols(&filtred_df, "Symbol")?);
            let lcosts = parse_incomes(&filtred_df, "Cost basis base currency")?;
            ta.stock
                .gross
                .extend(parse_incomes(&filtred_df, "Gross proceeds base currency")?);
            let fees = parse_incomes(&filtred_df, "Fees  base currency")?;

            // Add fees to costs
            let lcosts: Vec<crate::Currency> = lcosts
                .iter()
                .zip(fees)
                .map(|(x, y)| x.derive(x.value() + y.value()))
                .collect();
            ta.stock.costs.extend(lcosts);
        }
        ParsingState::DividendsEUR(s) | ParsingState::DividendsUSD(s) => {
            log::trace!("String to parse of Dividends: {s}");
            let df = CsvReader::new(std::io::Cursor::new(s.as_bytes()))
                .truncate_ragged_lines(true)
                .with_separator(delimiter)
                .finish()
                .map_err(|e| format!("Error reading CSV (Dividends): {e}"))?;
            log::info!("Content of Dividends: {df}");
            let filtred_df = extract_dividends_transactions(&df)?;
            log::info!("Filtered Dividend Data of interest: {filtred_df}");
            ta.dates
                .extend(parse_investment_transaction_dates(&filtred_df, "Date")?);

            ta.symbols.extend(parse_symbols(&filtred_df, "Symbol")?);

            // parse income
            let lincomes = parse_incomes(&filtred_df, "Gross amount base currency")?;
            // parse taxes
            let net = parse_incomes(&filtred_df, "Net amount base currency")?;

            // Add Tax in base currency is missing so We need
            // to calculate it based on net income e.g. gross - net = tax
            let ltaxes: Vec<crate::Currency> = lincomes
                .iter()
                .zip(net)
                .map(|(x, y)| x.derive(x.value() - y.value()))
                .collect();
            ta.incomes.extend(lincomes);
            ta.taxes.extend(ltaxes);
        }
        ParsingState::Crypto(s) => {
            log::trace!("String to parse of Crypto: {s}");
            let df = CsvReader::new(std::io::Cursor::new(s.as_bytes()))
                .truncate_ragged_lines(true)
                .with_separator(delimiter)
                .finish()
                .map_err(|e| format!("Error reading CSV (Crypto): {e}"))?;
            log::info!("Content of Crypto: {df}");
            let lacquired_dates = parse_investment_transaction_dates(&df, "Date acquired")?;
            log::trace!("acquired dates:: {:?}", lacquired_dates);
            let lsold_dates = parse_investment_transaction_dates(&df, "Date sold")?;
            log::trace!("sold dates:: {:?}", lsold_dates);
            // For each sold data has to be one acquire date
            if lacquired_dates.len() != lsold_dates.len() {
                return Err("ERROR: Different number of acquired and sold dates".to_string());
            }
            ta.crypto.sold_dates.extend(lsold_dates);
            ta.crypto.acquired_dates.extend(lacquired_dates);
            ta.crypto.costs.extend(parse_incomes(&df, "Cost basis")?);
            ta.crypto
                .gross
                .extend(parse_incomes(&df, "Gross proceeds")?);
        }
    }
    Ok(())
}

/// Parse revolut CSV documents (savings account, trading, crypto)
/// returns: (
/// dividend transactions in a form: date, gross income, tax taken, company name (if available)
/// sold transactions in a form date acquired, date sold, cost basis, gross income
/// crypto transactions in a form date acquired, date sold, cost basis, gross income
/// )
pub fn parse_revolut_transactions(csvtoparse: &str) -> Result<RevolutTransactions, String> {
    let mut dividend_transactions: Vec<(String, crate::Currency, crate::Currency, Option<String>)> =
        vec![];
    let mut sold_transactions: Vec<(
        String,
        String,
        crate::Currency,
        crate::Currency,
        Option<String>,
    )> = vec![];
    let mut crypto_transactions: Vec<(String, String, crate::Currency, crate::Currency)> = vec![];

    let mut ta = TransactionAccumulator::default();

    let original_delimiter: u8 = if csvtoparse.ends_with(".tsv") {
        b'\t'
    } else {
        b','
    };
    const DELIMITER: u8 = b';';
    const DELIMITER_AS_STR: &str = ";";

    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .delimiter(original_delimiter)
        .from_path(csvtoparse)
        .map_err(|_| "Error: opening CSV")?;

    let result = rdr
        .headers()
        .map_err(|e| format!("Error: scanning CSV header: {e}"))?;
    if result.iter().any(|field| field == "Completed Date") {
        log::info!("Detected Savings account statement: {csvtoparse}");
        let df = CsvReader::from_path(csvtoparse)
            .map_err(|_| "Error: opening CSV")?
            .with_separator(original_delimiter)
            .has_header(true)
            .finish()
            .map_err(|e| format!("Error reading CSV: {e}"))?;

        log::info!("CSV DataFrame: {df}");

        let filtred_df = extract_intrest_rate_transactions(&df)?;

        log::info!("Filtered data of Interest: {filtred_df}");

        ta.dates = parse_investment_transaction_dates(&filtred_df, "Completed Date")?;

        ta.incomes = parse_incomes(&filtred_df, "Money in")?;
        // Taxes are not automatically taken from savings account
        // so we will put zeros as tax taken
        ta.taxes = ta.incomes.iter().map(|i| i.derive(0.0)).collect();
        ta.symbols
            .extend(std::iter::repeat(None).take(ta.incomes.len()));
    } else if result.iter().any(|field| field == "Price per share") {
        log::info!("Detected Investment account statement: {csvtoparse}");
        let df = CsvReader::from_path(csvtoparse)
            .map_err(|_| "Error: opening CSV")?
            .with_separator(original_delimiter)
            .has_header(true)
            .finish()
            .map_err(|e| format!("Error reading CSV: {e}"))?;

        log::info!("CSV DataFrame: {df}");
        let filtred_df = extract_investment_gains_and_costs_transactions(&df)?;
        log::info!("Filtered Data of interest: {filtred_df}");
        ta.dates = parse_investment_transaction_dates(&filtred_df, "Date")?;
        ta.symbols = parse_symbols(&filtred_df, "Ticker")?;
        ta.incomes = parse_incomes(&filtred_df, "Total Amount")?;
        ta.taxes = ta.incomes.iter().map(|i| i.derive(0.0)).collect();
    } else if result.iter().any(|field| field == "Income from Sells") {
        let mut content1 = String::new();
        let mut content2 = String::new();
        let mut switch = false;
        for result in rdr.records() {
            let record = result.map_err(|e| format!("Error reading CSV: {e}"))?;
            let line = record
                .into_iter()
                .collect::<Vec<&str>>()
                .join(DELIMITER_AS_STR);
            if line.starts_with("Other income & fees") {
                switch = true;
            } else {
                if switch {
                    content2.push_str(&line);
                    content2.push('\n');
                } else {
                    content1.push_str(&line);
                    content1.push('\n');
                }
            }
        }
        log::info!("Content of first to be DataFrame: {content1}");
        log::info!("Content of second to be DataFrame: {content2}");

        let sales = CsvReader::new(std::io::Cursor::new(content1.as_bytes()))
            .with_separator(DELIMITER)
            .finish()
            .map_err(|e| format!("Error reading CSV: {e}"))?;

        let others = CsvReader::new(std::io::Cursor::new(content2.as_bytes()))
            .with_separator(DELIMITER)
            .truncate_ragged_lines(true)
            .finish()
            .map_err(|e| format!("Error reading CSV: {e}"))?;

        // Get rid of rows with empty data
        let mask = sales
            .get_columns()
            .iter()
            .map(|s| s.is_not_null())
            .fold(BooleanChunked::full("", false, sales.height()), |acc, b| {
                acc | b
            });

        let sales = sales
            .filter(&mask)
            .map_err(|e| format!("Error reading CSV: {e}"))?;
        log::info!("Content of first to be DataFrame: {sales}");

        let filtred_df = extract_sold_transactions(&sales)?;
        log::info!("Filtered Sold Data of interest: {filtred_df}");
        ta.stock.acquired_dates = parse_investment_transaction_dates(&filtred_df, "Date acquired")?;
        ta.stock.sold_dates = parse_investment_transaction_dates(&filtred_df, "Date sold")?;
        // For each sold date there has to be one acquire date
        if ta.stock.acquired_dates.len() != ta.stock.sold_dates.len() {
            return Err("ERROR: Different number of acquired and sold dates".to_string());
        }
        ta.stock.costs = parse_income_with_currency(&filtred_df, "Cost basis", "Currency")?;
        ta.stock.gross = parse_income_with_currency(&filtred_df, "Gross proceeds", "Currency")?;
        ta.stock.symbols = parse_symbols(&filtred_df, "Symbol")?;

        log::info!("Content of second to be DataFrame: {others}");

        let filtred_df = extract_dividends_transactions(&others)?;
        log::info!("Filtered Dividend Data of interest: {filtred_df}");
        ta.dates = parse_investment_transaction_dates(&filtred_df, "Date")?;
        // parse symbols of companies
        ta.symbols = parse_symbols(&filtred_df, "Symbol")?;

        // parse income
        ta.incomes = parse_income_with_currency(&filtred_df, "Gross amount", "Currency")?;
        // parse taxes
        ta.taxes = parse_income_with_currency(&filtred_df, "Withholding tax", "Currency")?;
    } else if result
        .iter()
        .any(|field| field.starts_with("Summary for") == true)
    {
        let mut state = ParsingState::None;

        for result in rdr.records() {
            let record = result.map_err(|e| format!("Error reading CSV record: {e}"))?;
            let line = record
                .into_iter()
                .collect::<Vec<&str>>()
                .join(DELIMITER_AS_STR);
            if line.starts_with("Transactions for") {
                process_tax_consolidated_data(&state, DELIMITER, &mut ta)?;

                if line.contains("Savings Accounts - EUR") {
                    log::info!("Starting to collect: EUR interests");
                    state = ParsingState::InterestsEUR(String::new());
                } else if line.contains("Savings Accounts - PLN") {
                    log::info!("Starting to collect: PLN interests");
                    state = ParsingState::InterestsPLN(String::new());
                } else if line.contains("Brokerage Account sells - EUR") {
                    log::info!("Starting to collect: EUR Sells");
                    state = ParsingState::SellEUR(String::new());
                } else if line.contains("Brokerage Account sells - USD") {
                    log::info!("Starting to collect: USD Sells");
                    state = ParsingState::SellUSD(String::new());
                } else if line.contains("Brokerage Account dividends - EUR") {
                    log::info!("Starting to collect: EUR dividends");
                    state = ParsingState::DividendsEUR(String::new());
                } else if line.contains("Brokerage Account dividends - USD") {
                    log::info!("Starting to collect: USD dividends");
                    state = ParsingState::DividendsUSD(String::new());
                } else if line.contains("Crypto") {
                    log::info!("Starting to collect: Crypto transactions");
                    state = ParsingState::Crypto(String::new());
                } else {
                    return Err("ERROR: Unsupported CSV type of document".to_string());
                }
            } else {
                match &mut state {
                    ParsingState::None => (),
                    ParsingState::SellEUR(s)
                    | ParsingState::SellUSD(s)
                    | ParsingState::DividendsEUR(s)
                    | ParsingState::DividendsUSD(s) => {
                        // Skip a line with info on protfolio creation
                        if line.contains("Portfolio") == false {
                            s.push_str(&line);
                            s.push('\n');
                        }
                    }
                    ParsingState::InterestsEUR(s)
                    | ParsingState::InterestsPLN(s)
                    | ParsingState::Crypto(s) => {
                        s.push_str(&line);
                        s.push('\n');
                    }
                }
            }
        }
        process_tax_consolidated_data(&state, DELIMITER, &mut ta)?;
    } else {
        return Err("ERROR: Unsupported CSV type of document: {csvtoparse}".to_string());
    }
    // Sold transactions
    log::info!("Sold Acquire Dates: {:?}", ta.stock.acquired_dates);
    log::info!("Sold Sold Dates: {:?}", ta.stock.sold_dates);
    log::info!("Sold Incomes: {:?}", ta.stock.gross);
    log::info!("Sold Cost Basis: {:?}", ta.stock.costs);

    if ta.stock.acquired_dates.len() != ta.stock.gross.len()
        || ta.stock.acquired_dates.len() != ta.stock.sold_dates.len()
        || ta.stock.acquired_dates.len() != ta.stock.symbols.len()
    {
        return Err(format!(
            "ERROR: Different number of sold acquired_dates({}), sold_dates({}), gross({}), cost({}) or symbols({})",
            ta.stock.acquired_dates.len(),
            ta.stock.sold_dates.len(),
            ta.stock.gross.len(),
            ta.stock.costs.len(),
            ta.stock.symbols.len()
        ));
    }

    let iter = std::iter::zip(
        ta.stock.acquired_dates,
        std::iter::zip(
            ta.stock.symbols,
            std::iter::zip(
                ta.stock.sold_dates,
                std::iter::zip(ta.stock.costs, ta.stock.gross),
            ),
        ),
    );
    iter.for_each(|(acq_d, (s, (sol_d, (c, g))))| {
        sold_transactions.push((acq_d, sol_d, c, g, s));
    });
    // Crypto transactions
    log::info!("Crypto Acquire Dates: {:?}", ta.crypto.acquired_dates);
    log::info!("Crypto Sold Dates: {:?}", ta.crypto.sold_dates);
    log::info!("Crypto Incomes: {:?}", ta.crypto.gross);
    log::info!("Crypto Cost Basis: {:?}", ta.crypto.costs);
    let iter = std::iter::zip(
        ta.crypto.acquired_dates,
        std::iter::zip(
            ta.crypto.sold_dates,
            std::iter::zip(ta.crypto.costs, ta.crypto.gross),
        ),
    );
    iter.for_each(|(acq_d, (sol_d, (c, g)))| {
        crypto_transactions.push((acq_d, sol_d, c, g));
    });

    // Dividends
    log::info!("Dividend Dates: {:?}", ta.dates);
    log::info!("Dividend Incomes: {:?}", ta.incomes);
    log::info!("Dividend Taxes: {:?}", ta.taxes);

    if ta.dates.len() != ta.incomes.len()
        || ta.dates.len() != ta.taxes.len()
        || ta.dates.len() != ta.symbols.len()
    {
        return Err(format!(
            "ERROR: Different number of dividend dates({}), incomes({}), taxes({}) or symbols({})",
            ta.dates.len(),
            ta.incomes.len(),
            ta.taxes.len(),
            ta.symbols.len()
        ));
    }

    let iter = std::iter::zip(
        ta.dates,
        std::iter::zip(ta.symbols, std::iter::zip(ta.incomes, ta.taxes)),
    );
    iter.for_each(|(d, (s, (m, t)))| {
        dividend_transactions.push((d, m, t, s));
    });
    Ok(RevolutTransactions {
        dividend_transactions,
        sold_transactions,
        crypto_transactions,
    })
}

mod tests {
    use super::*;

    #[test]
    fn test_extract_cash() -> Result<(), String> {
        assert_eq!(extract_cash("0,07€"), Ok(crate::Currency::EUR(0.07)));
        assert_eq!(extract_cash("6 000€"), Ok(crate::Currency::EUR(6000.00)));
        assert_eq!(extract_cash("600,34€"), Ok(crate::Currency::EUR(600.34)));

        assert_eq!(extract_cash("€840.03"), Ok(crate::Currency::EUR(840.03)));
        assert_eq!(extract_cash("€0.01"), Ok(crate::Currency::EUR(0.01)));
        assert_eq!(extract_cash("€440"), Ok(crate::Currency::EUR(440.0)));

        assert_eq!(extract_cash("1,06 PLN"), Ok(crate::Currency::PLN(1.06)));
        assert_eq!(
            extract_cash("500 000.45 PLN"),
            Ok(crate::Currency::PLN(500000.45))
        );
        assert_eq!(
            extract_cash("13,037.94 PLN"),
            Ok(crate::Currency::PLN(13037.94))
        );

        assert_eq!(extract_cash("$2.94"), Ok(crate::Currency::USD(2.94)));
        assert_eq!(extract_cash("-$0.51"), Ok(crate::Currency::USD(-0.51)));

        assert_eq!(extract_cash("63,28$"), Ok(crate::Currency::USD(63.28)));
        assert_eq!(extract_cash("0$"), Ok(crate::Currency::USD(0.0)));
        Ok(())
    }

    #[test]
    fn test_parse_incomes() -> Result<(), String> {
        let moneyin = Series::new("Money in", vec!["6000€", "3000€"]);
        let description = Series::new("Description", vec!["odsetki", "odsetki"]);

        let df =
            DataFrame::new(vec![description, moneyin]).map_err(|_| "Error creating DataFrame")?;

        assert_eq!(
            parse_incomes(&df, "Money in"),
            Ok(vec![
                crate::Currency::EUR(6000.00),
                crate::Currency::EUR(3000.00)
            ])
        );

        Ok(())
    }

    #[test]
    fn test_parse_incomes_pl() -> Result<(), String> {
        let moneyin = Series::new("Money in", vec!["0,27€", "5 452,74€"]);
        let description = Series::new("Description", vec!["odsetki", "odsetki"]);

        let df =
            DataFrame::new(vec![description, moneyin]).map_err(|_| "Error creating DataFrame")?;

        assert_eq!(
            parse_incomes(&df, "Money in"),
            Ok(vec![
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(5452.74)
            ])
        );

        Ok(())
    }

    #[test]
    fn test_parse_investment_incomes() -> Result<(), String> {
        let moneyin = Series::new("Total Amount", vec!["$2.94", "-$0.51"]);
        let description = Series::new("Description", vec!["DIVIDEND", "CUSTODY FEE"]);

        let df =
            DataFrame::new(vec![description, moneyin]).map_err(|_| "Error creating DataFrame")?;

        assert_eq!(
            parse_incomes(&df, "Total Amount"),
            Ok(vec![
                crate::Currency::USD(2.94),
                crate::Currency::USD(-0.51)
            ])
        );

        Ok(())
    }

    fn test_parse_date_helper(
        description: Vec<&str>,
        input_dates: Vec<&str>,
        expected_dates: Vec<String>,
    ) -> Result<(), String> {
        let description_series = Series::new("Description", description);
        let input_date_series = Series::new("Date", input_dates);

        let df = DataFrame::new(vec![description_series, input_date_series])
            .map_err(|_| "Error creating DataFrame")?;

        assert_eq!(
            parse_investment_transaction_dates(&df, "Date"),
            Ok(expected_dates)
        );

        Ok(())
    }

    #[test]
    fn test_parse_transaction_dates() -> Result<(), String> {
        let description = vec!["odsetki", "odsetki"];
        let input_dates = vec!["25 Aug 2023", "1 Sep 2023"];
        let expected_dates = vec!["08/25/23".to_string(), "09/01/23".to_string()];

        test_parse_date_helper(description, input_dates, expected_dates)
    }

    #[test]
    fn test_parse_symbols() -> Result<(), &'static str> {
        let dates = vec!["25 Aug 2023", "1 Sep 2023"];
        let symbols = vec!["AAPL", "MSFT"];
        let expected_symbols = symbols
            .iter()
            .map(|s| Some(s.to_string()))
            .collect::<Vec<Option<String>>>();

        let input_date_series = Series::new("Date", dates);
        let input_symbols = Series::new("Symbol", symbols);

        let df = DataFrame::new(vec![input_date_series, input_symbols])
            .map_err(|_| "Error creating DataFrame")?;

        assert_eq!(parse_symbols(&df, "Symbol"), Ok(expected_symbols));
        Ok(())
    }

    #[test]
    fn test_parse_transaction_dates_us() -> Result<(), String> {
        let description = vec!["odsetki", "odsetki"];
        let input_dates = vec!["Jan 3, 2024", "Dec 31, 2024"];
        let expected_dates = vec!["01/03/24".to_string(), "12/31/24".to_string()];

        test_parse_date_helper(description, input_dates, expected_dates)
    }

    #[test]
    fn test_parse_transaction_dates_uk() -> Result<(), String> {
        let description = vec!["odsetki", "odsetki"];
        let input_dates = vec!["7 Sept 2024", "10 Apr 2024"];
        let expected_dates = vec!["09/07/24".to_string(), "04/10/24".to_string()];

        test_parse_date_helper(description, input_dates, expected_dates)
    }

    #[test]
    fn test_parse_transaction_dates_pl() -> Result<(), String> {
        let description = vec!["odsetki", "odsetki"];
        let input_dates = vec!["25 sty 2023", "1 wrz 2023"];
        let expected_dates = vec!["01/25/23".to_string(), "09/01/23".to_string()];

        test_parse_date_helper(description, input_dates, expected_dates)
    }

    #[test]
    fn test_parse_investment_transaction_dates() -> Result<(), String> {
        let description = vec!["DIVIDEND", "CUSTODY FEE"];
        let input_dates = vec!["2023-12-08T14:30:08.150Z", "2023-09-09T05:35:43.253726Z"];
        let expected_dates = vec!["12/08/23".to_string(), "09/09/23".to_string()];

        test_parse_date_helper(description, input_dates, expected_dates)
    }

    #[test]
    fn test_parse_gain_and_losses_transaction_dates() -> Result<(), String> {
        let description = vec!["DIVIDEND", "CUSTODY FEE"];
        let input_dates = vec!["2024-03-04", "2024-07-16"];
        let expected_dates = vec!["03/04/24".to_string(), "07/16/24".to_string()];

        test_parse_date_helper(description, input_dates, expected_dates)
    }

    #[test]
    fn test_parse_revolut_transactions_consolidated_crypto() -> Result<(), String> {
        let expected_result = Ok(RevolutTransactions {
            dividend_transactions: vec![],
            sold_transactions: vec![],
            crypto_transactions: vec![
                (
                    "02/14/20".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(50.97),
                    crate::Currency::USD(63.28),
                ),
                (
                    "02/25/23".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.74),
                ),
                (
                    "02/25/23".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.37),
                ),
                (
                    "06/09/24".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.15),
                ),
                (
                    "06/09/24".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.16),
                ),
                (
                    "06/09/24".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.13),
                ),
                (
                    "06/09/24".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.13),
                ),
                (
                    "06/09/24".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.12),
                ),
                (
                    "06/09/24".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.14),
                ),
                (
                    "06/09/24".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.14),
                ),
                (
                    "06/09/24".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.14),
                ),
                (
                    "06/09/24".to_owned(),
                    "12/06/24".to_owned(),
                    crate::Currency::USD(0.0),
                    crate::Currency::USD(0.15),
                ),
            ],
        });

        assert_eq!(
            parse_revolut_transactions("revolut_data/crypt.csv"),
            expected_result
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_transactions_consolidated_crypto_tsv() -> Result<(), String> {
        // This test verifies that the consolidated TSV with crypto transactions
        // is parsed and that crypto cost basis and gross proceeds are counted
        // According to the summary present in the file (Gross proceeds = 7.95$, Cost basis = 0$).
        let res = parse_revolut_transactions("revolut_data/consolidated-eur-pln-crypto_2025.tsv");
        if res.is_err() {
            return Err(format!("Parsing failed: {:?}", res));
        }
        let parsed = res.unwrap();

        // There should be some crypto transactions parsed
        assert!(
            !parsed.crypto_transactions.is_empty(),
            "No crypto transactions parsed"
        );

        // Sum up cost basis and gross proceeds (values are stored as Currency)
        let total_cost: f64 = parsed
            .crypto_transactions
            .iter()
            .map(|(_, _, cost, _)| cost.value())
            .sum();
        let total_gross: f64 = parsed
            .crypto_transactions
            .iter()
            .map(|(_, _, _, gross)| gross.value())
            .sum();

        // The consolidated summary in the fixture reports 0$ cost and 7.95$ gross
        let eps = 1e-6;
        assert!(
            (total_cost - 0.0).abs() < eps,
            "expected total crypto cost ~0.0, got {}",
            total_cost
        );
        assert!(
            (total_gross - 7.95).abs() < eps,
            "expected total crypto gross ~7.95, got {}",
            total_gross
        );

        // Verify savings interest totals (present in the file summary):
        // EUR interests total = 1,66€ and PLN interests total = 10,09 PLN
        let mut sum_eur = 0.0f64;
        let mut sum_pln = 0.0f64;
        parsed
            .dividend_transactions
            .iter()
            .for_each(|(_, amount, _, _)| match amount {
                crate::Currency::EUR(v) => sum_eur += v,
                crate::Currency::PLN(v) => sum_pln += v,
                _ => (),
            });

        let eps = 1e-6;
        assert!(
            (sum_eur - 1.66).abs() < eps,
            "expected EUR interests ~1.66, got {}",
            sum_eur
        );
        assert!(
            (sum_pln - 10.09).abs() < eps,
            "expected PLN interests ~10.09, got {}",
            sum_pln
        );

        // Sold transactions: this consolidated file doesn't include brokerage sells
        assert_eq!(
            parsed.sold_transactions.len(),
            0,
            "expected no sold transactions"
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_transactions_consolidated_eur() -> Result<(), String> {
        let expected_result = Ok(RevolutTransactions {
            dividend_transactions: vec![
                // EUR interests
                (
                    "01/03/24".to_owned(),
                    crate::Currency::EUR(0.01),
                    crate::Currency::EUR(0.00),
                    None,
                ),
                (
                    "01/04/24".to_owned(),
                    crate::Currency::EUR(0.02),
                    crate::Currency::EUR(0.00),
                    None,
                ),
                (
                    "12/31/24".to_owned(),
                    crate::Currency::EUR(0.01),
                    crate::Currency::EUR(0.00),
                    None,
                ),
            ],
            sold_transactions: vec![],
            crypto_transactions: vec![],
        });
        assert_eq!(
            parse_revolut_transactions("revolut_data/consolidated-eur_2024.csv"),
            expected_result
        );
        Ok(())
    }

    #[test]
    fn test_parse_revolut_transactions_consolidated() -> Result<(), String> {
        let expected_result = Ok(RevolutTransactions {
            dividend_transactions: vec![
                // EUR interests
                (
                    "01/01/24".to_owned(),
                    crate::Currency::EUR(0.26),
                    crate::Currency::EUR(0.00),
                    None,
                ),
                (
                    "04/12/24".to_owned(),
                    crate::Currency::EUR(0.24),
                    crate::Currency::EUR(0.00),
                    None,
                ),
                // PLN interests
                (
                    "01/04/24".to_owned(),
                    crate::Currency::PLN(0.86),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "05/31/24".to_owned(),
                    crate::Currency::PLN(1.26),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                // Euro dividends
                (
                    "08/26/24".to_owned(),
                    crate::Currency::PLN(302.43),
                    crate::Currency::PLN(302.43 - 222.65),
                    Some("DE000A289XJ2".to_string()),
                ),
                // USD dividends
                (
                    "03/04/24".to_owned(),
                    crate::Currency::PLN(617.00),
                    crate::Currency::PLN(617.00 - 524.43),
                    Some("TFC".to_string()),
                ),
                (
                    "03/21/24".to_owned(),
                    crate::Currency::PLN(259.17),
                    crate::Currency::PLN(0.0),
                    Some("AMCR".to_string()),
                ),
                (
                    "12/17/24".to_owned(),
                    crate::Currency::PLN(903.35),
                    crate::Currency::PLN(903.35 - 767.83),
                    Some("EPR".to_string()),
                ),
            ],
            sold_transactions: vec![
                (
                    "07/29/24".to_owned(),
                    "10/28/24".to_owned(),
                    crate::Currency::PLN(13037.94 + 65.94),
                    crate::Currency::PLN(13348.22),
                    Some("EU000A3K4DJ5".to_string()),
                ),
                (
                    "09/09/24".to_owned(),
                    "11/21/24".to_owned(),
                    crate::Currency::PLN(16097.86 + 81.41),
                    crate::Currency::PLN(16477.91),
                    Some("XS1218821756".to_string()),
                ),
                (
                    "11/20/23".to_owned(),
                    "08/12/24".to_owned(),
                    crate::Currency::PLN(19863.25 + 0.66),
                    crate::Currency::PLN(22865.17),
                    Some("XOM".to_string()),
                ),
                (
                    "06/11/24".to_owned(),
                    "10/14/24".to_owned(),
                    crate::Currency::PLN(525.08 + 0.0),
                    crate::Currency::PLN(624.00),
                    Some("TFC".to_string()),
                ),
                (
                    "10/23/23".to_owned(),
                    "10/14/24".to_owned(),
                    crate::Currency::PLN(835.88 + 0.03),
                    crate::Currency::PLN(1046.20),
                    Some("AMCR".to_string()),
                ),
                (
                    "08/22/24".to_owned(),
                    "10/17/24".to_owned(),
                    crate::Currency::PLN(25135.50 + 128.17),
                    crate::Currency::PLN(26130.41),
                    Some("US13607LNF66".to_string()),
                ),
            ],
            crypto_transactions: vec![],
        });
        assert_eq!(
            parse_revolut_transactions("revolut_data/consolidated-statement_2024.csv"),
            expected_result
        );
        Ok(())
    }

    #[test]
    fn test_parse_revolut_investment_gain_and_losses_dividends() -> Result<(), String> {
        let expected_result = Ok(RevolutTransactions {
            dividend_transactions: vec![
                (
                    "06/04/24".to_owned(),
                    crate::Currency::PLN(2.80),
                    crate::Currency::PLN(0.68),
                    Some("QDVY".to_string()),
                ),
                (
                    "06/20/24".to_owned(),
                    crate::Currency::PLN(0.34),
                    crate::Currency::PLN(0.08),
                    Some("EXI2".to_string()),
                ),
                (
                    "06/28/24".to_owned(),
                    crate::Currency::PLN(3.79),
                    crate::Currency::PLN(0.94),
                    Some("IS3K".to_string()),
                ),
                (
                    "07/01/24".to_owned(),
                    crate::Currency::PLN(1.07),
                    crate::Currency::PLN(0.25),
                    Some("IBCD".to_string()),
                ),
            ],
            sold_transactions: vec![],
            crypto_transactions: vec![],
        });

        assert_eq!(
            parse_revolut_transactions("revolut_data/trading-pnl-statement_2024-01-robo.csv"),
            expected_result
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_investment_with_commas_gain_and_losses_dividends() -> Result<(), String> {
        let expected_result = Ok(RevolutTransactions {
            dividend_transactions: vec![
                (
                    "06/04/24".to_owned(),
                    crate::Currency::PLN(2.80),
                    crate::Currency::PLN(0.68),
                    Some("QDVY".to_string()),
                ),
                (
                    "06/20/24".to_owned(),
                    crate::Currency::PLN(0.34),
                    crate::Currency::PLN(0.08),
                    Some("EXI2".to_string()),
                ),
                (
                    "06/28/24".to_owned(),
                    crate::Currency::PLN(3.79),
                    crate::Currency::PLN(0.94),
                    Some("IS3K".to_string()),
                ),
                (
                    "07/01/24".to_owned(),
                    crate::Currency::PLN(1.07),
                    crate::Currency::PLN(0.25),
                    Some("IBCD".to_string()),
                ),
                (
                    "09/27/24".to_owned(),
                    crate::Currency::PLN(1.02),
                    crate::Currency::PLN(0.25),
                    Some("IBCD".to_string()),
                ),
                (
                    "09/27/24".to_owned(),
                    crate::Currency::PLN(1.71),
                    crate::Currency::PLN(0.42),
                    Some("IUSU".to_string()),
                ),
                (
                    "11/29/24".to_owned(),
                    crate::Currency::PLN(2.92),
                    crate::Currency::PLN(0.73),
                    Some("QDVY".to_string()),
                ),
                (
                    "12/17/24".to_owned(),
                    crate::Currency::PLN(0.04),
                    crate::Currency::PLN(0.0),
                    Some("EXI2".to_string()),
                ),
                (
                    "12/31/24".to_owned(),
                    crate::Currency::PLN(1.07),
                    crate::Currency::PLN(0.25),
                    Some("IBCD".to_string()),
                ),
            ],
            sold_transactions: vec![],
            crypto_transactions: vec![],
        });

        assert_eq!(
            parse_revolut_transactions("revolut_data/trading-pnl-statement_2024-01-robo-2.csv"),
            expected_result
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_investment_gain_and_losses_sells_and_dividends() -> Result<(), String> {
        let expected_result = Ok(RevolutTransactions {
            dividend_transactions: vec![
                (
                    "03/04/24".to_owned(),
                    crate::Currency::PLN(617.00),
                    crate::Currency::PLN(92.57),
                    Some("TFC".to_string()),
                ),
                (
                    "03/21/24".to_owned(),
                    crate::Currency::PLN(259.17),
                    crate::Currency::PLN(0.0),
                    Some("AMCR".to_string()),
                ),
                (
                    "03/25/24".to_owned(),
                    crate::Currency::PLN(212.39),
                    crate::Currency::PLN(31.87),
                    Some("PXD".to_string()),
                ),
                (
                    "05/16/24".to_owned(),
                    crate::Currency::PLN(700.17),
                    crate::Currency::PLN(105.04),
                    Some("EPR".to_string()),
                ),
                (
                    "05/31/24".to_owned(),
                    crate::Currency::PLN(875.82),
                    crate::Currency::PLN(131.38),
                    Some("UPS".to_string()),
                ),
                (
                    "06/03/24".to_owned(),
                    crate::Currency::PLN(488.26),
                    crate::Currency::PLN(73.25),
                    Some("ABR".to_string()),
                ),
                (
                    "06/04/24".to_owned(),
                    crate::Currency::PLN(613.2),
                    crate::Currency::PLN(92.00),
                    Some("TFC".to_string()),
                ),
                (
                    "06/11/24".to_owned(),
                    crate::Currency::PLN(186.16),
                    crate::Currency::PLN(27.92),
                    Some("XOM".to_string()),
                ),
                (
                    "06/13/24".to_owned(),
                    crate::Currency::PLN(264.74),
                    crate::Currency::PLN(0.00),
                    Some("AMCR".to_string()),
                ),
                (
                    "06/18/24".to_owned(),
                    crate::Currency::PLN(858.33),
                    crate::Currency::PLN(128.74),
                    Some("EPR".to_string()),
                ),
                (
                    "07/12/24".to_owned(),
                    crate::Currency::PLN(421.5),
                    crate::Currency::PLN(63.23),
                    Some("BBY".to_string()),
                ),
                (
                    "07/16/24".to_owned(),
                    crate::Currency::PLN(834.55),
                    crate::Currency::PLN(125.18),
                    Some("EPR".to_string()),
                ),
                (
                    "08/16/24".to_owned(),
                    crate::Currency::PLN(834.79),
                    crate::Currency::PLN(125.23),
                    Some("EPR".to_string()),
                ),
                (
                    "08/26/24".to_owned(),
                    crate::Currency::PLN(302.43),
                    crate::Currency::PLN(79.77),
                    Some("DE000A289XJ2".to_string()),
                ),
                (
                    "08/29/24".to_owned(),
                    crate::Currency::PLN(801.25),
                    crate::Currency::PLN(0.0),
                    Some("BMO".to_string()),
                ),
                (
                    "08/30/24".to_owned(),
                    crate::Currency::PLN(872.56),
                    crate::Currency::PLN(130.90),
                    Some("CAG".to_string()),
                ),
            ],
            sold_transactions: vec![(
                "11/20/23".to_owned(),
                "08/12/24".to_owned(),
                crate::Currency::USD(5000.0),
                crate::Currency::USD(5804.62),
                Some("XOM".to_string()),
            )],
            crypto_transactions: vec![],
        });

        assert_eq!(
            parse_revolut_transactions(
                "revolut_data/trading-pnl-statement_2022-11-01_2024-09-01_pl-pl_e989f4.csv"
            ),
            expected_result
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_transactions_english_statement_pln() -> Result<(), String> {
        let expected_result = Ok(RevolutTransactions {
            dividend_transactions: vec![
                (
                    "12/12/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/13/23".to_owned(),
                    crate::Currency::PLN(0.20),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/15/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/16/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/17/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/18/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/19/23".to_owned(),
                    crate::Currency::PLN(0.41),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/20/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/21/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/22/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/23/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/24/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/25/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/26/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/27/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/28/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/29/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/30/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
                (
                    "12/31/23".to_owned(),
                    crate::Currency::PLN(0.21),
                    crate::Currency::PLN(0.00),
                    None,
                ),
            ],
            sold_transactions: vec![],
            crypto_transactions: vec![],
        });
        assert_eq!(
            parse_revolut_transactions("revolut_data/revolut-savings-eng.csv"),
            expected_result
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_investment_transactions_usd() -> Result<(), String> {
        let expected_result = Ok(RevolutTransactions {
            dividend_transactions: vec![
                (
                    "11/02/23".to_owned(),
                    crate::Currency::USD(-0.02),
                    crate::Currency::USD(0.00),
                    None,
                ),
                (
                    "12/01/23".to_owned(),
                    crate::Currency::USD(-0.51),
                    crate::Currency::USD(0.00),
                    None,
                ),
                (
                    "12/14/23".to_owned(),
                    crate::Currency::USD(2.94),
                    crate::Currency::USD(0.00),
                    Some("AMCR".to_string()),
                ),
            ],
            sold_transactions: vec![],
            crypto_transactions: vec![],
        });
        assert_eq!(
            parse_revolut_transactions("revolut_data/revolut_div.csv"),
            expected_result
        );
        Ok(())
    }
}
