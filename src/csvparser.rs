pub use crate::logging::ResultExt;
use nom::{
    branch::alt,
    bytes::complete::is_a,
    bytes::complete::tag,
    bytes::complete::take,
    bytes::complete::take_till,
    bytes::complete::take_until,
    bytes::complete::take_while,
    character::{complete::alphanumeric1, is_digit},
    combinator::peek,
    error::Error,
    number::complete::double,
    sequence::delimited,
    sequence::tuple,
    IResult,
};
use polars::prelude::*;

fn extract_cash(cashline: &str) -> Result<crate::Currency, &'static str> {
    // We need to erase "," before processing it by parser
    log::info!("Entry moneyin line: {cashline}");
    let cashline_string: String = cashline.to_string().replace(",", "");
    log::info!("Processed moneyin line: {cashline_string}");
    let mut euro_parser = tuple((tag("+€"), double::<&str, Error<_>>));
    let mut pln_parser = tuple((tag("+"), double::<&str, Error<_>>, take(1usize), tag("PLN")));

    match euro_parser(cashline_string.as_str()) {
        Ok((_, (_, value))) => return Ok(crate::Currency::EUR(value)),
        Err(_) => match pln_parser(cashline_string.as_str()) {
            Ok((_, (_, value, _, _))) => return Ok(crate::Currency::PLN(value)),
            Err(_) => return Err("Error converting: {cashline_string}"),
        },
    }
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
                    if x.contains("Odsetki brutto") {
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
        .map_err(|_| "Error: Unable to select Complete Date")?;
    let mut dates: Vec<String> = vec![];
    let possible_dates = date
        .utf8()
        .map_err(|_| "Error: Unable to convert to utf8")?;
    possible_dates.into_iter().try_for_each(|x| {
        if let Some(d) = x {
            let cd = chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%dT%H:%M:%S%.fZ")
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

fn parse_incomes(df: DataFrame) -> Result<Vec<crate::Currency>, &'static str> {
    let mut incomes: Vec<crate::Currency> = vec![];
    let moneyin = df
        .column("Money in")
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

pub fn parse_revolut_transactions(
    csvtoparse: &str,
) -> Result<Vec<(String, crate::Currency)>, &str> {
    let df = CsvReader::from_path(csvtoparse)
        .map_err(|_| "Error: opening CSV")?
        .has_header(true)
        .finish()
        .map_err(|_| "Error: opening CSV")?;

    log::info!("CSV DataFrame: {df}");

    // TODO: If there is interest rate transactions then proceed with this path

    let mut transactions: Vec<(String, crate::Currency)> = vec![];

    if df
        .select(&["Completed Date", "Description", "Money in"])
        .is_ok()
    {
        log::info!("Detected Savings account statement: {csvtoparse}");

        let filtred_df = extract_intrest_rate_transactions(&df)?;

        log::info!("Filtered data of Interest: {filtred_df}");

        let dates = parse_transaction_dates(&filtred_df)?;
        log::info!("Dates: {:?}", dates);

        let incomes = parse_incomes(filtred_df)?;
        log::info!("Incomes: {:?}", incomes);

        let iter = std::iter::zip(dates, incomes);
        iter.for_each(|(d, m)| {
            transactions.push((d, m));
        });
    } else if df.select(&["Type", "Price per share"]).is_ok() {
        log::info!("Detected Investment account statement: {csvtoparse}");
        let filtred_df = extract_investment_gains_and_costs_transactions(&df)?;
        log::info!("Filtered Data of interest: {filtred_df}");
    } else {
        return Err("ERROR: Unsupported CSV type of document: {csvtoparse}");
    }
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

        Ok(())
    }

    #[test]
    fn test_parse_incomes() -> Result<(), String> {
        let moneyin = Series::new("Money in", vec!["+€6,000", "+€3,000"]);
        let description = Series::new("Description", vec!["odsetki", "odsetki"]);

        let df =
            DataFrame::new(vec![description, moneyin]).map_err(|_| "Error creating DataFrame")?;

        assert_eq!(
            parse_incomes(df),
            Ok(vec![
                crate::Currency::EUR(6000.00),
                crate::Currency::EUR(3000.00)
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
    fn test_parse_revolut_transactions_eur() -> Result<(), String> {
        let expected_result = Ok(vec![
            ("08/24/23".to_owned(), crate::Currency::EUR(0.05)),
            ("08/25/23".to_owned(), crate::Currency::EUR(0.07)),
            ("08/26/23".to_owned(), crate::Currency::EUR(0.06)),
            ("08/27/23".to_owned(), crate::Currency::EUR(0.06)),
            ("08/28/23".to_owned(), crate::Currency::EUR(0.06)),
            ("08/29/23".to_owned(), crate::Currency::EUR(0.06)),
            ("08/30/23".to_owned(), crate::Currency::EUR(0.06)),
            ("08/31/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/01/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/02/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/03/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/04/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/05/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/06/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/07/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/08/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/09/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/10/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/11/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/12/23".to_owned(), crate::Currency::EUR(0.06)),
            ("09/13/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/14/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/15/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/16/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/17/23".to_owned(), crate::Currency::EUR(0.25)),
            ("09/18/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/19/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/20/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/21/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/22/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/23/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/24/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/25/23".to_owned(), crate::Currency::EUR(0.25)),
            ("09/26/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/27/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/28/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/29/23".to_owned(), crate::Currency::EUR(0.24)),
            ("09/30/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/01/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/02/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/03/23".to_owned(), crate::Currency::EUR(0.25)),
            ("10/04/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/05/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/06/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/07/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/08/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/09/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/10/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/11/23".to_owned(), crate::Currency::EUR(0.25)),
            ("10/12/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/13/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/14/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/15/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/16/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/17/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/18/23".to_owned(), crate::Currency::EUR(0.25)),
            ("10/19/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/20/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/21/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/22/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/23/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/24/23".to_owned(), crate::Currency::EUR(0.25)),
            ("10/25/23".to_owned(), crate::Currency::EUR(0.25)),
            ("10/26/23".to_owned(), crate::Currency::EUR(0.25)),
            ("10/27/23".to_owned(), crate::Currency::EUR(0.24)),
            ("10/28/23".to_owned(), crate::Currency::EUR(0.25)),
            ("10/29/23".to_owned(), crate::Currency::EUR(0.25)),
            ("10/30/23".to_owned(), crate::Currency::EUR(0.25)),
            ("10/31/23".to_owned(), crate::Currency::EUR(0.24)),
            ("11/01/23".to_owned(), crate::Currency::EUR(0.25)),
            ("11/02/23".to_owned(), crate::Currency::EUR(0.25)),
            ("11/03/23".to_owned(), crate::Currency::EUR(0.25)),
            ("11/04/23".to_owned(), crate::Currency::EUR(0.24)),
            ("11/05/23".to_owned(), crate::Currency::EUR(0.25)),
            ("11/06/23".to_owned(), crate::Currency::EUR(0.25)),
            ("11/07/23".to_owned(), crate::Currency::EUR(0.25)),
            ("11/08/23".to_owned(), crate::Currency::EUR(0.24)),
            ("11/09/23".to_owned(), crate::Currency::EUR(0.25)),
            ("11/10/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/11/23".to_owned(), crate::Currency::EUR(0.26)),
            ("11/12/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/13/23".to_owned(), crate::Currency::EUR(0.26)),
            ("11/14/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/15/23".to_owned(), crate::Currency::EUR(0.26)),
            ("11/16/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/17/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/18/23".to_owned(), crate::Currency::EUR(0.26)),
            ("11/19/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/20/23".to_owned(), crate::Currency::EUR(0.26)),
            ("11/21/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/22/23".to_owned(), crate::Currency::EUR(0.26)),
            ("11/23/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/24/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/25/23".to_owned(), crate::Currency::EUR(0.26)),
            ("11/26/23".to_owned(), crate::Currency::EUR(0.27)),
            ("11/27/23".to_owned(), crate::Currency::EUR(0.26)),
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
            ("08/29/23".to_owned(), crate::Currency::PLN(0.44)),
            ("08/30/23".to_owned(), crate::Currency::PLN(0.45)),
            ("08/31/23".to_owned(), crate::Currency::PLN(0.44)),
            ("09/01/23".to_owned(), crate::Currency::PLN(0.45)),
            ("09/02/23".to_owned(), crate::Currency::PLN(0.44)),
            ("09/03/23".to_owned(), crate::Currency::PLN(0.44)),
            ("09/04/23".to_owned(), crate::Currency::PLN(0.45)),
            ("09/05/23".to_owned(), crate::Currency::PLN(0.77)),
            ("09/06/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/07/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/08/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/09/23".to_owned(), crate::Currency::PLN(0.77)),
            ("09/10/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/11/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/12/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/13/23".to_owned(), crate::Currency::PLN(0.77)),
            ("09/14/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/15/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/16/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/17/23".to_owned(), crate::Currency::PLN(0.78)),
            ("09/18/23".to_owned(), crate::Currency::PLN(0.77)),
            ("09/19/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/20/23".to_owned(), crate::Currency::PLN(1.01)),
            ("09/21/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/22/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/23/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/24/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/25/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/26/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/27/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/28/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/29/23".to_owned(), crate::Currency::PLN(1.0)),
            ("09/30/23".to_owned(), crate::Currency::PLN(1.0)),
            ("10/01/23".to_owned(), crate::Currency::PLN(1.01)),
            ("10/02/23".to_owned(), crate::Currency::PLN(1.0)),
            ("10/03/23".to_owned(), crate::Currency::PLN(1.0)),
            ("10/04/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/05/23".to_owned(), crate::Currency::PLN(1.05)),
            ("10/06/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/07/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/08/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/09/23".to_owned(), crate::Currency::PLN(1.05)),
            ("10/10/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/11/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/12/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/13/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/14/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/15/23".to_owned(), crate::Currency::PLN(1.05)),
            ("10/16/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/17/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/18/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/19/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/20/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/21/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/22/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/23/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/24/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/25/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/26/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/27/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/28/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/29/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/30/23".to_owned(), crate::Currency::PLN(1.06)),
            ("10/31/23".to_owned(), crate::Currency::PLN(1.06)),
            ("11/01/23".to_owned(), crate::Currency::PLN(1.06)),
            ("11/02/23".to_owned(), crate::Currency::PLN(1.06)),
            ("11/03/23".to_owned(), crate::Currency::PLN(1.06)),
            ("11/04/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/05/23".to_owned(), crate::Currency::PLN(1.11)),
            ("11/06/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/07/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/08/23".to_owned(), crate::Currency::PLN(1.11)),
            ("11/09/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/10/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/11/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/12/23".to_owned(), crate::Currency::PLN(1.11)),
            ("11/13/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/14/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/15/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/16/23".to_owned(), crate::Currency::PLN(1.11)),
            ("11/17/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/18/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/19/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/20/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/21/23".to_owned(), crate::Currency::PLN(1.12)),
            ("11/22/23".to_owned(), crate::Currency::PLN(0.82)),
            ("11/23/23".to_owned(), crate::Currency::PLN(0.83)),
            ("11/24/23".to_owned(), crate::Currency::PLN(0.83)),
            ("11/25/23".to_owned(), crate::Currency::PLN(0.83)),
            ("11/26/23".to_owned(), crate::Currency::PLN(0.83)),
            ("11/27/23".to_owned(), crate::Currency::PLN(0.83)),
        ]);
        assert_eq!(
            parse_revolut_transactions("revolut_data/Revolut_30cze2023_27lis2023.csv"),
            expected_result
        );

        Ok(())
    }
}
