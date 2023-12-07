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

#[derive(Debug, PartialEq)]
pub enum Currency {
    PLN(f64),
    EUR(f64),
}

fn extract_cash(cashline: &str) -> Result<Currency, &'static str> {
    // We need to erase "," before processing it by parser
    log::info!("Entry moneyin line: {cashline}");
    let cashline_string: String = cashline.to_string().replace(",", "");
    log::info!("Processed moneyin line: {cashline_string}");
    let mut euro_parser = tuple((tag("+€"), double::<&str, Error<_>>));
    let mut pln_parser = tuple((tag("+"), double::<&str, Error<_>>, take(1usize), tag("PLN")));

    match euro_parser(cashline_string.as_str()) {
        Ok((_, (_, value))) => return Ok(Currency::EUR(value)),
        Err(_) => match pln_parser(cashline_string.as_str()) {
            Ok((_, (_, value, _, _))) => return Ok(Currency::PLN(value)),
            Err(_) => return Err("Error converting: {cashline_string}"),
        },
    }
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

fn parse_transaction_dates(df: &DataFrame) -> Result<Vec<chrono::NaiveDate>, &'static str> {
    let completed_date = df
        .column("Completed Date")
        .map_err(|_| "Error: Unable to select Complete Date")?;
    let mut dates: Vec<chrono::NaiveDate> = vec![];
    let possible_dates = completed_date
        .utf8()
        .map_err(|_| "Error: Unable to convert to utf8")?;
    possible_dates.into_iter().for_each(|x| {
        if let Some(d) = x {
            let cd = chrono::NaiveDate::parse_from_str(&d, "%e %b %Y")
                .expect("Error converting cell to NaiveDate");

            dates.push(cd);
        }
    });

    Ok(dates)
}

fn parse_incomes(df: DataFrame) -> Result<Vec<Currency>, &'static str> {
    let mut incomes: Vec<Currency> = vec![];
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
) -> Result<Vec<(chrono::NaiveDate, Currency)>, &str> {
    let df = CsvReader::from_path(csvtoparse)
        .map_err(|_| "Error: opening CSV")?
        .has_header(true)
        .finish()
        .map_err(|_| "Error: opening CSV")?;

    log::info!("CSV DataFrame: {df}");

    let filtred_df = extract_intrest_rate_transactions(&df)?;

    log::info!("DF: {filtred_df}");

    let dates = parse_transaction_dates(&filtred_df)?;
    log::info!("Dates: {:?}", dates);

    let incomes = parse_incomes(filtred_df)?;
    println!("Incomes: {:?}", incomes);

    let mut transactions: Vec<(chrono::NaiveDate, Currency)> = vec![];
    let mut iter = std::iter::zip(dates, incomes);
    iter.for_each(|(d, m)| {
        transactions.push((d, m));
    });

    Ok(transactions)
}

mod tests {
    use super::*;

    #[test]
    fn test_extract_cash() -> Result<(), String> {
        assert_eq!(extract_cash("+€0.07"), Ok(Currency::EUR(0.07)));
        assert_eq!(extract_cash("+€6,000"), Ok(Currency::EUR(6000.00)));
        assert_eq!(extract_cash("+€600"), Ok(Currency::EUR(600.00)));
        assert_eq!(extract_cash("+€6,000.45"), Ok(Currency::EUR(6000.45)));

        assert_eq!(extract_cash("+1.06 PLN"), Ok(Currency::PLN(1.06)));
        assert_eq!(extract_cash("+4,000 PLN"), Ok(Currency::PLN(4000.00)));
        assert_eq!(extract_cash("+500 PLN"), Ok(Currency::PLN(500.00)));
        assert_eq!(extract_cash("+4,000.32 PLN"), Ok(Currency::PLN(4000.32)));

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
            Ok(vec![Currency::EUR(6000.00), Currency::EUR(3000.00)])
        );

        Ok(())
    }
}
