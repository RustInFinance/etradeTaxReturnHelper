use clap::{App, AppSettings, Arg};

mod de;
mod logging;
mod pl;
mod us;
use etradeTaxReturnHelper::run_taxation;
use logging::ResultExt;

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
                .help("Brokerage statement PDFs  and Gain & Losses xlsx documents\n\nBrokerege statements can be downloaded from:\n\thttps://edoc.etrade.com/e/t/onlinedocs/docsearch?doc_type=stmt\n\nGain&Losses documents can be downloaded from:\n\thttps://us.etrade.com/etx/sp/stockplan#/myAccount/gainsLosses\n")
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
        .expect_and_log("error getting brokarage statements pdfs names.\n\nBrokerege statements can be downloaded from:\n\nhttps://edoc.etrade.com/e/t/onlinedocs/docsearch?doc_type=stmt\n\n");

    let (gross_div, tax_div, gross_sold, cost_sold) = run_taxation(&rd, pdfnames).unwrap();

    rd.present_result(gross_div, tax_div, gross_sold, cost_sold);
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{App, Arg, ArgMatches, ErrorKind, Values};

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
    #[ignore]
    fn test_dividends_taxation() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = App::new("E-trade tax helper").setting(AppSettings::ArgRequiredElseHelp);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});
        // Check printed values or returned values?
        let matches = create_cmd_line_pattern(myapp).get_matches_from_safe(vec![
            "mytest",
            "data/Brokerage Statement - XXXX0848 - 202102.pdf",
            "data/Brokerage Statement - XXXX0848 - 202103.pdf",
            "data/Brokerage Statement - XXXX0848 - 202105.pdf",
            "data/Brokerage Statement - XXXX0848 - 202106.pdf",
            "data/Brokerage Statement - XXXX0848 - 202108.pdf",
            "data/Brokerage Statement - XXXX0848 - 202109.pdf",
            "data/Brokerage Statement - XXXX0848 - 202112.pdf",
        ])?;
        let pdfnames = matches
            .values_of("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        assert_eq!(
            etradeTaxReturnHelper::run_taxation(&rd, pdfnames),
            Ok((9674.047, 1451.0844, 0.0, 0.0))
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_sold_dividends_taxation() -> Result<(), clap::Error> {
        // Get all brokerage with dividends only
        let myapp = App::new("E-trade tax helper").setting(AppSettings::ArgRequiredElseHelp);
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(pl::PL {});
        let matches = create_cmd_line_pattern(myapp).get_matches_from_safe(vec![
            "mytest",
            "data/Brokerage Statement - XXXX0848 - 202202.pdf",
            "data/Brokerage Statement - XXXX0848 - 202203.pdf",
            "data/Brokerage Statement - XXXX0848 - 202204.pdf",
            "data/G&L_Collapsed.xlsx",
        ])?;
        let pdfnames = matches
            .values_of("financial documents")
            .expect_and_log("error getting brokarage statements pdfs names");
        assert_eq!(
            etradeTaxReturnHelper::run_taxation(&rd, pdfnames),
            Ok((2930.206, 439.54138, 201.32295, 0.0))
        );
        Ok(())
    }
}
