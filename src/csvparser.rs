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
) -> Result<Vec<(chrono::NaiveDate, crate::Currency)>, &str> {
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
    log::info!("Incomes: {:?}", incomes);

    let mut transactions: Vec<(chrono::NaiveDate, crate::Currency)> = vec![];
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
        assert_eq!(extract_cash("+€0.07"), Ok(crate::Currency::EUR(0.07)));
        assert_eq!(extract_cash("+€6,000"), Ok(crate::Currency::EUR(6000.00)));
        assert_eq!(extract_cash("+€600"), Ok(crate::Currency::EUR(600.00)));
        assert_eq!(extract_cash("+€6,000.45"), Ok(crate::Currency::EUR(6000.45)));

        assert_eq!(extract_cash("+1.06 PLN"), Ok(crate::Currency::PLN(1.06)));
        assert_eq!(extract_cash("+4,000 PLN"), Ok(crate::Currency::PLN(4000.00)));
        assert_eq!(extract_cash("+500 PLN"), Ok(crate::Currency::PLN(500.00)));
        assert_eq!(extract_cash("+4,000.32 PLN"), Ok(crate::Currency::PLN(4000.32)));

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
            Ok(vec![crate::Currency::EUR(6000.00), crate::Currency::EUR(3000.00)])
        );

        Ok(())
    }

    #[test]
    fn test_parse_transaction_dates() -> Result<(), String> {
        let completed_dates = Series::new("Completed Date", vec!["25 Aug 2023", "1 Sep 2023"]);
        let description = Series::new("Description", vec!["odsetki", "odsetki"]);

        let df = DataFrame::new(vec![description, completed_dates])
            .map_err(|_| "Error creating DataFrame")?;

        let expected_first_date =
            chrono::NaiveDate::parse_from_str("25 Aug 2023", "%e %b %Y").unwrap();
        let expected_second_date =
            chrono::NaiveDate::parse_from_str("1 Sep 2023", "%e %b %Y").unwrap();

        assert_eq!(
            parse_transaction_dates(&df),
            Ok(vec![expected_first_date, expected_second_date])
        );

        Ok(())
    }

    #[test]
    fn test_parse_revolut_transactions_eur() -> Result<(), String> {
        let expected_result = Ok(vec![
            (
                chrono::NaiveDate::parse_from_str("24 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.05),
            ),
            (
                chrono::NaiveDate::parse_from_str("25 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.07),
            ),
            (
                chrono::NaiveDate::parse_from_str("26 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("27 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("28 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("29 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("30 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("31 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("1 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("2 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("3 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("4 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("5 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("6 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("7 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("8 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("9 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("10 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("11 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("12 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("13 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("14 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("15 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("16 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("17 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("18 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("19 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("20 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("21 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("22 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("23 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("24 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("25 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("26 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("27 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("28 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("29 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("30 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("1 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("2 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("3 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("4 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("5 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("6 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("7 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("8 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("9 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("10 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("11 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("12 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("13 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("14 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("15 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("16 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("17 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("18 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("19 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("20 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("21 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("22 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("23 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("24 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("25 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("26 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("27 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("28 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("29 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("30 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("31 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("1 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("2 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("3 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("4 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("5 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("6 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("7 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("8 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.24),
            ),
            (
                chrono::NaiveDate::parse_from_str("9 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.25),
            ),
            (
                chrono::NaiveDate::parse_from_str("10 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("11 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.26),
            ),
            (
                chrono::NaiveDate::parse_from_str("12 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("13 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.26),
            ),
            (
                chrono::NaiveDate::parse_from_str("14 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("15 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.26),
            ),
            (
                chrono::NaiveDate::parse_from_str("16 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("17 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("18 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.26),
            ),
            (
                chrono::NaiveDate::parse_from_str("19 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("20 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.26),
            ),
            (
                chrono::NaiveDate::parse_from_str("21 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("22 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.26),
            ),
            (
                chrono::NaiveDate::parse_from_str("23 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("24 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("25 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.26),
            ),
            (
                chrono::NaiveDate::parse_from_str("26 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.27),
            ),
            (
                chrono::NaiveDate::parse_from_str("27 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::EUR(0.26),
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
                chrono::NaiveDate::parse_from_str("29 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.44),
            ),
            (
                chrono::NaiveDate::parse_from_str("30 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.45),
            ),
            (
                chrono::NaiveDate::parse_from_str("31 Aug 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.44),
            ),
            (
                chrono::NaiveDate::parse_from_str("1 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.45),
            ),
            (
                chrono::NaiveDate::parse_from_str("2 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.44),
            ),
            (
                chrono::NaiveDate::parse_from_str("3 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.44),
            ),
            (
                chrono::NaiveDate::parse_from_str("4 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.45),
            ),
            (
                chrono::NaiveDate::parse_from_str("5 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.77),
            ),
            (
                chrono::NaiveDate::parse_from_str("6 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("7 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("8 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("9 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.77),
            ),
            (
                chrono::NaiveDate::parse_from_str("10 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("11 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("12 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("13 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.77),
            ),
            (
                chrono::NaiveDate::parse_from_str("14 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("15 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("16 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("17 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.78),
            ),
            (
                chrono::NaiveDate::parse_from_str("18 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.77),
            ),
            (
                chrono::NaiveDate::parse_from_str("19 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("20 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.01),
            ),
            (
                chrono::NaiveDate::parse_from_str("21 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("22 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("23 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("24 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("25 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("26 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("27 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("28 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("29 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("30 Sep 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("1 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.01),
            ),
            (
                chrono::NaiveDate::parse_from_str("2 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("3 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.0),
            ),
            (
                chrono::NaiveDate::parse_from_str("4 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("5 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.05),
            ),
            (
                chrono::NaiveDate::parse_from_str("6 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("7 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("8 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("9 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.05),
            ),
            (
                chrono::NaiveDate::parse_from_str("10 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("11 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("12 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("13 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("14 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("15 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.05),
            ),
            (
                chrono::NaiveDate::parse_from_str("16 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("17 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("18 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("19 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("20 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("21 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("22 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("23 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("24 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("25 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("26 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("27 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("28 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("29 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("30 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("31 Oct 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("1 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("2 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("3 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.06),
            ),
            (
                chrono::NaiveDate::parse_from_str("4 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("5 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.11),
            ),
            (
                chrono::NaiveDate::parse_from_str("6 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("7 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("8 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.11),
            ),
            (
                chrono::NaiveDate::parse_from_str("9 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("10 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("11 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("12 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.11),
            ),
            (
                chrono::NaiveDate::parse_from_str("13 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("14 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("15 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("16 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.11),
            ),
            (
                chrono::NaiveDate::parse_from_str("17 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("18 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("19 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("20 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("21 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(1.12),
            ),
            (
                chrono::NaiveDate::parse_from_str("22 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.82),
            ),
            (
                chrono::NaiveDate::parse_from_str("23 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.83),
            ),
            (
                chrono::NaiveDate::parse_from_str("24 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.83),
            ),
            (
                chrono::NaiveDate::parse_from_str("25 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.83),
            ),
            (
                chrono::NaiveDate::parse_from_str("26 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.83),
            ),
            (
                chrono::NaiveDate::parse_from_str("27 Nov 2023", "%e %b %Y").unwrap(),
                crate::Currency::PLN(0.83),
            ),
        ]);
        assert_eq!(
            parse_revolut_transactions("revolut_data/Revolut_30cze2023_27lis2023.csv"),
            expected_result
        );

        Ok(())
    }
}
