pub use crate::logging::ResultExt;
use nom::{
    branch::alt,
    bytes::complete::tag,
    bytes::complete::take,
    character::{complete::alphanumeric1, is_digit},
    error::Error,
    multi::many_m_n,
    number::complete::double,
    sequence::delimited,
    sequence::tuple,
    IResult,
};
use polars::prelude::*;

fn extract_cash_with_currency(cashline: &str, currency: &str) -> Result<crate::Currency, String> {
    log::info!("Entry cacheline: {cashline}");
    log::info!("Entry currency: {currency}");

    println!("Entry cacheline: {cashline}");
    println!("Entry currency: {currency}");

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

fn extract_cash(cashline: &str) -> Result<crate::Currency, &'static str> {
    // We need to erase "," before processing it by parser
    log::info!("Entry moneyin/total amount line: {cashline}");
    let cashline_string: String = cashline.to_string().replace(",", "");
    log::info!("Processed moneyin/total amount line: {cashline_string}");
    let mut euro_parser = tuple((tag("+€"), double::<&str, Error<_>>));
    let mut usd_parser = tuple((many_m_n(0, 1, tag("-")), tag("$"), double::<&str, Error<_>>));
    let mut pln_parser = tuple((tag("+"), double::<&str, Error<_>>, take(1usize), tag("PLN")));

    match euro_parser(cashline_string.as_str()) {
        Ok((_, (_, value))) => return Ok(crate::Currency::EUR(value)),
        Err(_) => match pln_parser(cashline_string.as_str()) {
            Ok((_, (_, value, _, _))) => return Ok(crate::Currency::PLN(value)),
            Err(_) => match usd_parser(cashline_string.as_str()) {
                Ok((_, (sign, _, value))) => {
                    if sign.len() == 1 {
                        return Ok(crate::Currency::USD(-value));
                    } else {
                        return Ok(crate::Currency::USD(value));
                    }
                }
                Err(_) => return Err("Error converting: {cashline_string}"),
            },
        },
    }
}

fn extract_dividends_transactions(df: &DataFrame) -> Result<DataFrame, &'static str> {
    let mut df_transactions = df
        .select(&["Date", "Gross amount", "Withholding tax", "Currency"])
        .map_err(|_| "Error: Unable to select dividend data")?;

    Ok(df_transactions)
}

fn extract_investment_gains_and_costs_transactions(
    df: &DataFrame,
) -> Result<DataFrame, &'static str> {
    let mut df_transactions = df
        .select(&["Date", "Type", "Total Amount"])
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
    let mut df_transactions = df
        .select(&["Completed Date", "Description", "Money in"])
        .map_err(|_| "Error: Unable to select description")?;

    let intrest_rate = df_transactions
        .column("Description")
        .map_err(|_| "Error: Unable to get Description")?
        .iter()
        .map(|x| {
            let m = match x {
                AnyValue::Utf8(x) => {
                    if x.contains("Odsetki brutto") || x.contains("Gross interest") {
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

fn parse_investment_transaction_dates(df: &DataFrame) -> Result<Vec<String>, &'static str> {
    let date = df
        .column("Date")
        .map_err(|_| "Error: Unable to select Date")?;
    let mut dates: Vec<String> = vec![];
    let possible_dates = date
        .utf8()
        .map_err(|_| "Error: Unable to convert to utf8")?;
    possible_dates.into_iter().try_for_each(|x| {
        if let Some(d) = x {
            let cd = chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%dT%H:%M:%S%.fZ")
                .or_else(|_| chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d"))
                .map_err(|_| "Error converting cell to NaiveDate")?
                .format("%m/%d/%y")
                .to_string();
            dates.push(cd);
        }
        Ok::<(), &str>(())
    })?;

    Ok(dates)
}

fn parse_transaction_dates(df: &DataFrame) -> Result<Vec<String>, &'static str> {
    let completed_date = df
        .column("Completed Date")
        .map_err(|_| "Error: Unable to select Complete Date")?;
    let mut dates: Vec<String> = vec![];
    let possible_dates = completed_date
        .utf8()
        .map_err(|_| "Error: Unable to convert to utf8")?;
    possible_dates.into_iter().try_for_each(|x| {
        if let Some(d) = x {
            let cd = chrono::NaiveDate::parse_from_str(&d, "%e %b %Y")
                .map_err(|_| "Error converting cell to NaiveDate")?
                .format("%m/%d/%y")
                .to_string();
            dates.push(cd);
        }
        Ok::<(), &str>(())
    })?;

    Ok(dates)
}

fn parse_incomes(df: DataFrame, col: &str) -> Result<Vec<crate::Currency>, &'static str> {
    let mut incomes: Vec<crate::Currency> = vec![];
    let moneyin = df
        .column(col)
        .map_err(|_| "Error: Unable to select Money In")?;
    let possible_incomes = moneyin
        .utf8()
        .map_err(|_| "Error: Unable to convert to utf8")?;
    possible_incomes.into_iter().try_for_each(|x| {
        if let Some(d) = x {
            incomes.push(extract_cash(d)?);
        }
        Ok::<(), &str>(())
    })?;
    Ok(incomes)
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

/// Parse revolut CSV documents (savings account and trading)
/// returns: transactions in a form: date, gross income , tax taken
pub fn parse_revolut_transactions(
    csvtoparse: &str,
) -> Result<Vec<(String, crate::Currency, crate::Currency)>, String> {
    let mut transactions: Vec<(String, crate::Currency, crate::Currency)> = vec![];

    let mut dates: Vec<String> = vec![];
    let mut incomes: Vec<crate::Currency> = vec![];
    let mut taxes: Vec<crate::Currency> = vec![];
    //let mut rdr = csv::Reader::from_path(csvtoparse).map_err(|_| "Error: opening CSV")?;
    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(csvtoparse)
        .map_err(|_| "Error: opening CSV")?;
    let result = rdr
        .headers()
        .map_err(|e| format!("Error: scanning CSV header: {e}"))?;
    if result.iter().any(|field| field == "Completed Date") {
        log::info!("Detected Savings account statement: {csvtoparse}");
        let df = CsvReader::from_path(csvtoparse)
            .map_err(|_| "Error: opening CSV")?
            .has_header(true)
            .finish()
            .map_err(|e| format!("Error reading CSV: {e}"))?;

        log::info!("CSV DataFrame: {df}");

        let filtred_df = extract_intrest_rate_transactions(&df)?;

        log::info!("Filtered data of Interest: {filtred_df}");

        dates = parse_transaction_dates(&filtred_df)?;

        incomes = parse_incomes(filtred_df, "Money in")?;
        // Taxes are not automatically taken from savings account
        // so we will put zeros as tax taken
        taxes = incomes.iter().map(|i| i.derive(0.0)).collect();
    } else if result.iter().any(|field| field == "Price per share") {
        log::info!("Detected Investment account statement: {csvtoparse}");
        let df = CsvReader::from_path(csvtoparse)
            .map_err(|_| "Error: opening CSV")?
            .has_header(true)
            .finish()
            .map_err(|e| format!("Error reading CSV: {e}"))?;

        log::info!("CSV DataFrame: {df}");
        let filtred_df = extract_investment_gains_and_costs_transactions(&df)?;
        log::info!("Filtered Data of interest: {filtred_df}");
        dates = parse_investment_transaction_dates(&filtred_df)?;
        incomes = parse_incomes(filtred_df, "Total Amount")?;
        taxes = incomes.iter().map(|i| i.derive(0.0)).collect();
    } else if result.iter().any(|field| field == "Income from Sells") {
        let mut content1 = String::new();
        let mut content2 = String::new();
        let mut switch = false;
        for result in rdr.records() {
            let record = result.map_err(|e| format!("Error reading CSV: {e}"))?;
            let line = record.into_iter().collect::<Vec<&str>>().join(",");
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
            .finish()
            .map_err(|e| format!("Error reading CSV: {e}"))?;

        let others = CsvReader::new(std::io::Cursor::new(content2.as_bytes()))
            .finish()
            .map_err(|e| format!("Error reading CSV: {e}"))?;

        log::info!("Content of first to be DataFrame: {sales}");
        log::info!("Content of second to be DataFrame: {others}");

        println!("Content of second to be DataFrame: {others}");

        let filtred_df = extract_dividends_transactions(&others)?;
        log::info!("Filtered Data of interest: {filtred_df}");
        dates = parse_investment_transaction_dates(&filtred_df)?;
        // parse income
        incomes = parse_income_with_currency(&filtred_df, "Gross amount", "Currency")?;
        // parse taxes
        taxes = parse_income_with_currency(&filtred_df, "Withholding tax", "Currency")?;

        // TODO: sales
    } else {
        return Err("ERROR: Unsupported CSV type of document: {csvtoparse}".to_string());
    }

    log::info!("Investment/Fees Dates: {:?}", dates);
    log::info!("Incomes: {:?}", incomes);
    log::info!("Taxes: {:?}", taxes);
    let iter = std::iter::zip(dates, std::iter::zip(incomes, taxes));
    iter.for_each(|(d, (m, t))| {
        transactions.push((d, m, t));
    });
    Ok(transactions)
}

mod tests {
    use super::*;

    #[test]
    fn test_extract_cash() -> Result<(), String> {
        assert_eq!(extract_cash("+€0.07"), Ok(crate::Currency::EUR(0.07)));
        assert_eq!(extract_cash("+€6,000"), Ok(crate::Currency::EUR(6000.00)));
        assert_eq!(extract_cash("+€600"), Ok(crate::Currency::EUR(600.00)));
        assert_eq!(
            extract_cash("+€6,000.45"),
            Ok(crate::Currency::EUR(6000.45))
        );

        assert_eq!(extract_cash("+1.06 PLN"), Ok(crate::Currency::PLN(1.06)));
        assert_eq!(
            extract_cash("+4,000 PLN"),
            Ok(crate::Currency::PLN(4000.00))
        );
        assert_eq!(extract_cash("+500 PLN"), Ok(crate::Currency::PLN(500.00)));
        assert_eq!(
            extract_cash("+4,000.32 PLN"),
            Ok(crate::Currency::PLN(4000.32))
        );

        assert_eq!(extract_cash("$2.94"), Ok(crate::Currency::USD(2.94)));
        assert_eq!(extract_cash("-$0.51"), Ok(crate::Currency::USD(-0.51)));
        Ok(())
    }

    #[test]
    fn test_parse_incomes() -> Result<(), String> {
        let moneyin = Series::new("Money in", vec!["+€6,000", "+€3,000"]);
        let description = Series::new("Description", vec!["odsetki", "odsetki"]);

        let df =
            DataFrame::new(vec![description, moneyin]).map_err(|_| "Error creating DataFrame")?;

        assert_eq!(
            parse_incomes(df, "Money in"),
            Ok(vec![
                crate::Currency::EUR(6000.00),
                crate::Currency::EUR(3000.00)
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
            parse_incomes(df, "Total Amount"),
            Ok(vec![
                crate::Currency::USD(2.94),
                crate::Currency::USD(-0.51)
            ])
        );

        Ok(())
    }

    #[test]
    fn test_parse_transaction_dates() -> Result<(), String> {
        let completed_dates = Series::new("Completed Date", vec!["25 Aug 2023", "1 Sep 2023"]);
        let description = Series::new("Description", vec!["odsetki", "odsetki"]);

        let df = DataFrame::new(vec![description, completed_dates])
            .map_err(|_| "Error creating DataFrame")?;

        let expected_first_date = "08/25/23".to_owned();
        let expected_second_date = "09/01/23".to_owned();

        assert_eq!(
            parse_transaction_dates(&df),
            Ok(vec![expected_first_date, expected_second_date])
        );

        Ok(())
    }

    #[test]
    fn test_parse_investment_transaction_dates() -> Result<(), String> {
        let completed_dates = Series::new(
            "Date",
            vec!["2023-12-08T14:30:08.150Z", "2023-09-09T05:35:43.253726Z"],
        );
        let description = Series::new("Type", vec!["DIVIDEND", "CUSTODY FEE"]);

        let df = DataFrame::new(vec![description, completed_dates])
            .map_err(|_| "Error creating DataFrame")?;

        let expected_first_date = "12/08/23".to_owned();
        let expected_second_date = "09/09/23".to_owned();

        assert_eq!(
            parse_investment_transaction_dates(&df),
            Ok(vec![expected_first_date, expected_second_date])
        );

        Ok(())
    }

    #[test]
    fn test_parse_gain_and_losses_transaction_dates() -> Result<(), String> {
        let completed_dates = Series::new("Date", vec!["2024-03-04", "2024-07-16"]);
        let description = Series::new("Type", vec!["DIVIDEND", "CUSTODY FEE"]);

        let df = DataFrame::new(vec![description, completed_dates])
            .map_err(|_| "Error creating DataFrame")?;

        let expected_first_date = "03/04/24".to_owned();
        let expected_second_date = "07/16/24".to_owned();

        assert_eq!(
            parse_investment_transaction_dates(&df),
            Ok(vec![expected_first_date, expected_second_date])
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_transactions_eur() -> Result<(), String> {
        let expected_result = Ok(vec![
            (
                "08/24/23".to_owned(),
                crate::Currency::EUR(0.05),
                crate::Currency::EUR(0.00),
            ),
            (
                "08/25/23".to_owned(),
                crate::Currency::EUR(0.07),
                crate::Currency::EUR(0.00),
            ),
            (
                "08/26/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "08/27/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "08/28/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "08/29/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "08/30/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "08/31/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/01/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/02/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/03/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/04/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/05/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/06/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/07/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/08/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/09/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/10/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/11/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/12/23".to_owned(),
                crate::Currency::EUR(0.06),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/13/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/14/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/15/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/16/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/17/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/18/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/19/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/20/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/21/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/22/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/23/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/24/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/25/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/26/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/27/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/28/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/29/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "09/30/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/01/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/02/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/03/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/04/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/05/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/06/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/07/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/08/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/09/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/10/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/11/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/12/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/13/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/14/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/15/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/16/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/17/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/18/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/19/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/20/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/21/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/22/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/23/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/24/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/25/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/26/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/27/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/28/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/29/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/30/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "10/31/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/01/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/02/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/03/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/04/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/05/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/06/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/07/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/08/23".to_owned(),
                crate::Currency::EUR(0.24),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/09/23".to_owned(),
                crate::Currency::EUR(0.25),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/10/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/11/23".to_owned(),
                crate::Currency::EUR(0.26),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/12/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/13/23".to_owned(),
                crate::Currency::EUR(0.26),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/14/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/15/23".to_owned(),
                crate::Currency::EUR(0.26),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/16/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/17/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/18/23".to_owned(),
                crate::Currency::EUR(0.26),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/19/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/20/23".to_owned(),
                crate::Currency::EUR(0.26),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/21/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/22/23".to_owned(),
                crate::Currency::EUR(0.26),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/23/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/24/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/25/23".to_owned(),
                crate::Currency::EUR(0.26),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/26/23".to_owned(),
                crate::Currency::EUR(0.27),
                crate::Currency::EUR(0.00),
            ),
            (
                "11/27/23".to_owned(),
                crate::Currency::EUR(0.26),
                crate::Currency::EUR(0.00),
            ),
        ]);

        assert_eq!(
            parse_revolut_transactions("revolut_data/Revolut_21sie2023_27lis2023.csv"),
            expected_result
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_transactions_pln() -> Result<(), String> {
        let expected_result = Ok(vec![
            (
                "08/29/23".to_owned(),
                crate::Currency::PLN(0.44),
                crate::Currency::PLN(0.00),
            ),
            (
                "08/30/23".to_owned(),
                crate::Currency::PLN(0.45),
                crate::Currency::PLN(0.00),
            ),
            (
                "08/31/23".to_owned(),
                crate::Currency::PLN(0.44),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/01/23".to_owned(),
                crate::Currency::PLN(0.45),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/02/23".to_owned(),
                crate::Currency::PLN(0.44),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/03/23".to_owned(),
                crate::Currency::PLN(0.44),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/04/23".to_owned(),
                crate::Currency::PLN(0.45),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/05/23".to_owned(),
                crate::Currency::PLN(0.77),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/06/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/07/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/08/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/09/23".to_owned(),
                crate::Currency::PLN(0.77),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/10/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/11/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/12/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/13/23".to_owned(),
                crate::Currency::PLN(0.77),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/14/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/15/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/16/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/17/23".to_owned(),
                crate::Currency::PLN(0.78),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/18/23".to_owned(),
                crate::Currency::PLN(0.77),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/19/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/20/23".to_owned(),
                crate::Currency::PLN(1.01),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/21/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/22/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/23/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/24/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/25/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/26/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/27/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/28/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/29/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "09/30/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/01/23".to_owned(),
                crate::Currency::PLN(1.01),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/02/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/03/23".to_owned(),
                crate::Currency::PLN(1.0),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/04/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/05/23".to_owned(),
                crate::Currency::PLN(1.05),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/06/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/07/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/08/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/09/23".to_owned(),
                crate::Currency::PLN(1.05),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/10/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/11/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/12/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/13/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/14/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/15/23".to_owned(),
                crate::Currency::PLN(1.05),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/16/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/17/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/18/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/19/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/20/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/21/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/22/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/23/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/24/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/25/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/26/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/27/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/28/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/29/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/30/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "10/31/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/01/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/02/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/03/23".to_owned(),
                crate::Currency::PLN(1.06),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/04/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/05/23".to_owned(),
                crate::Currency::PLN(1.11),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/06/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/07/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/08/23".to_owned(),
                crate::Currency::PLN(1.11),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/09/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/10/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/11/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/12/23".to_owned(),
                crate::Currency::PLN(1.11),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/13/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/14/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/15/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/16/23".to_owned(),
                crate::Currency::PLN(1.11),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/17/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/18/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/19/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/20/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/21/23".to_owned(),
                crate::Currency::PLN(1.12),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/22/23".to_owned(),
                crate::Currency::PLN(0.82),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/23/23".to_owned(),
                crate::Currency::PLN(0.83),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/24/23".to_owned(),
                crate::Currency::PLN(0.83),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/25/23".to_owned(),
                crate::Currency::PLN(0.83),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/26/23".to_owned(),
                crate::Currency::PLN(0.83),
                crate::Currency::PLN(0.00),
            ),
            (
                "11/27/23".to_owned(),
                crate::Currency::PLN(0.83),
                crate::Currency::PLN(0.00),
            ),
        ]);
        assert_eq!(
            parse_revolut_transactions("revolut_data/Revolut_30cze2023_27lis2023.csv"),
            expected_result
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_investment_gain_and_losses() -> Result<(), String> {
        let expected_result = Ok(vec![
            (
                "03/04/24".to_owned(),
                crate::Currency::PLN(617.00),
                crate::Currency::PLN(92.57),
            ),
            (
                "03/21/24".to_owned(),
                crate::Currency::PLN(259.17),
                crate::Currency::PLN(0.0),
            ),
            (
                "03/25/24".to_owned(),
                crate::Currency::PLN(212.39),
                crate::Currency::PLN(31.87),
            ),
            (
                "05/16/24".to_owned(),
                crate::Currency::PLN(700.17),
                crate::Currency::PLN(105.04),
            ),
            (
                "05/31/24".to_owned(),
                crate::Currency::PLN(875.82),
                crate::Currency::PLN(131.38),
            ),
            (
                "06/03/24".to_owned(),
                crate::Currency::PLN(488.26),
                crate::Currency::PLN(73.25),
            ),
            (
                "06/04/24".to_owned(),
                crate::Currency::PLN(613.2),
                crate::Currency::PLN(92.00),
            ),
            (
                "06/11/24".to_owned(),
                crate::Currency::PLN(186.16),
                crate::Currency::PLN(27.92),
            ),
            (
                "06/13/24".to_owned(),
                crate::Currency::PLN(264.74),
                crate::Currency::PLN(0.00),
            ),
            (
                "06/18/24".to_owned(),
                crate::Currency::PLN(858.33),
                crate::Currency::PLN(128.74),
            ),
            (
                "07/12/24".to_owned(),
                crate::Currency::PLN(421.5),
                crate::Currency::PLN(63.23),
            ),
            (
                "07/16/24".to_owned(),
                crate::Currency::PLN(834.55),
                crate::Currency::PLN(125.18),
            ),
        ]);

        assert_eq!(
            parse_revolut_transactions(
                "revolut_data/trading-pnl-statement_2024-01-01_2024-08-04_pl-pl_8e8783.csv"
            ),
            expected_result
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_transactions_english_statement_pln() -> Result<(), String> {
        let expected_result = Ok(vec![
            (
                "12/12/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/13/23".to_owned(),
                crate::Currency::PLN(0.20),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/15/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/16/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/17/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/18/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/19/23".to_owned(),
                crate::Currency::PLN(0.41),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/20/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/21/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/22/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/23/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/24/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/25/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/26/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/27/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/28/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/29/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/30/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
            (
                "12/31/23".to_owned(),
                crate::Currency::PLN(0.21),
                crate::Currency::PLN(0.00),
            ),
        ]);
        assert_eq!(
            parse_revolut_transactions("revolut_data/revolut-savings-eng.csv"),
            expected_result
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_investment_transactions_usd() -> Result<(), String> {
        let expected_result = Ok(vec![
            (
                "11/02/23".to_owned(),
                crate::Currency::USD(-0.02),
                crate::Currency::USD(0.00),
            ),
            (
                "12/01/23".to_owned(),
                crate::Currency::USD(-0.51),
                crate::Currency::USD(0.00),
            ),
            (
                "12/14/23".to_owned(),
                crate::Currency::USD(2.94),
                crate::Currency::USD(0.00),
            ),
        ]);
        assert_eq!(
            parse_revolut_transactions("revolut_data/revolut_div.csv"),
            expected_result
        );
        Ok(())
    }
}
