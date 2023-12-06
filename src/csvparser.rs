use polars::prelude::*;
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

#[derive(Debug, PartialEq)]
enum Currency {
    PLN(f64),
    EUR(f64),
}


fn extract_cash(cashline: &str) -> Currency {
    // We need to erase "," before processing it by parser
    log::info!("Entry moneyin line: {cashline}");
    let cashline_string : String = cashline.to_string().replace(",","");
    log::info!("Processed moneyin line: {cashline_string}");
    let mut euro_parser = tuple((tag("+€"), double::<&str,Error<_>>));
    let mut pln_parser = tuple((tag("+"), double::<&str,Error<_>>,take(1usize),tag("PLN")));

    match euro_parser(cashline_string.as_str()) {
        Ok((_,(_,value))) => return Currency::EUR(value),
        Err(_) => {
            match pln_parser(cashline) {
                Ok((_,(_,value,_,_))) => return Currency::PLN(value),
                Err(_) => panic!("Error converting: {cashline}") 
            }
        },
    }
}

pub fn parse_revolut_transactions(csvtoparse : &str) {

    let mut df = CsvReader::from_path(csvtoparse).map_err(|_| "Error: opening CSV").expect_and_log("TODO: propagate up").has_header(true).finish().map_err(|_| "Error: opening CSV").expect_and_log(&format!("Error opening CSV file: {}", csvtoparse));

    todo!();
}

mod tests {
    use super::*;

    #[test]
    fn test_extract_cash() -> Result<(), String> {

        assert_eq!(extract_cash("+€0.07"),Currency::EUR(0.07));
        assert_eq!(extract_cash("+€6,000"),Currency::EUR(6000.00));
        assert_eq!(extract_cash("+€600"),Currency::EUR(600.00));
        assert_eq!(extract_cash("+€6,000.45"),Currency::EUR(6000.45));

        assert_eq!(extract_cash("+1.06 PLN"),Currency::PLN(1.06));
        assert_eq!(extract_cash("+4,000 PLN"),Currency::PLN(4000.00));
        assert_eq!(extract_cash("+500 PLN"),Currency::PLN(500.00));
        assert_eq!(extract_cash("+4,000.32 PLN"),Currency::PLN(4000.32));

        Ok(())
    }
}
