use chrono;
use pdf::file::File;
use pdf::primitive::Primitive;
use serde::{Deserialize, Serialize};

enum ParserState {
    SearchingDividendEntry,
    SearchingINTCEntry,
    SearchingTaxEntry,
    SearchingGrossEntry,
}

struct Transaction {
    transaction_date: String,
    gross_us: f32,
    tax_us: f32,
    exchange_rate_date: String,
    exchange_rate: f32,
}

type ReqwestClient = reqwest::blocking::Client;

// Example response: {"table":"A",
//                    "currency":"dolar amerykański",
//                    "code":"USD",
//                    "rates":[{"no":"039/A/NBP/2021",
//                              "effectiveDate":"2021-02-26",
//                              "mid":3.7247}]}

#[derive(Debug, Deserialize, Serialize)]
struct NBPResponse<T> {
    table: String,
    currency: String,
    code: String,
    rates: Vec<T>,
}

#[derive(Debug, Deserialize, Serialize)]
struct ExchangeRate {
    no: String,
    effectiveDate: String,
    mid: f32,
}

fn init_logging_infrastructure() {
    // TODO(jczaja): test on windows/macos
    syslog::init(
        syslog::Facility::LOG_USER,
        log::LevelFilter::Debug,
        Some("corporate-assistant"),
    )
    .expect("Error initializing syslog");
}

fn get_exchange_rate(transaction_date: &str) -> Result<(String, f32), String> {
    // TODO: proxies
    let http_proxy: Option<&str> = Some("http://proxy-chain.intel.com:911");
    // If there is proxy then pick first URL
    let client = match http_proxy {
        Some(proxy) => ReqwestClient::builder()
            .proxy(reqwest::Proxy::http(proxy).expect("Error setting HTTP proxy"))
            .proxy(reqwest::Proxy::https(proxy).expect("Error setting HTTPS proxy"))
            .build()
            .expect("Could not create REST API client"),
        None => ReqwestClient::builder()
            .build()
            .expect("Could not create REST API client"),
    };

    let base_exchange_rate_url = "http://api.nbp.pl/api/exchangerates/rates/a/usd/";
    let mut converted_date =
        chrono::NaiveDate::parse_from_str(transaction_date, "%m/%d/%y").unwrap();

    // Try to get exchange rate going backwards with dates till success
    let mut is_success = false;
    let mut exchange_rate = 0.0;
    let mut exchange_rate_date: String = "N/A".to_string();
    while is_success == false {
        converted_date = converted_date
            .checked_sub_signed(chrono::Duration::days(1))
            .expect("Error traversing date");

        let exchange_rate_url: String = base_exchange_rate_url.to_string()
            + &format!("{}", converted_date.format("%Y-%m-%d"))
            + "/?format=json";

        let body = client.get(&(exchange_rate_url)).send();
        let actual_body = body.expect(&format!(
            "Getting Exchange Rate from NBP ({}) failed",
            exchange_rate_url
        ));
        is_success = actual_body.status().is_success();
        if is_success == true {
            log::info!("RESPONSE {:#?}", actual_body);

            let nbp_response = actual_body
                .json::<NBPResponse<ExchangeRate>>()
                .expect("Error converting response to JSON");
            log::info!("body of exchange_rate = {:#?}", nbp_response);
            exchange_rate = nbp_response.rates[0].mid;
            exchange_rate_date = format!("{}", converted_date.format("%Y-%m-%d"));
        }
    }

    Ok((exchange_rate_date, exchange_rate))
}

fn parse_brokerage_statement(pdftoparse: &str) -> Result<(String, f32, f32), String> {
    //2. parsing each pdf
    let mypdffile = File::<Vec<u8>>::open(pdftoparse).unwrap();

    let mut state = ParserState::SearchingDividendEntry;
    let mut transaction_date: String = "N/A".to_string();
    let mut tax_us = 0.0;

    log::info!("Parsing: {}", pdftoparse);
    for page in mypdffile.pages() {
        let page = page.unwrap();
        let contents = page.contents.as_ref().unwrap();
        for op in contents.operations.iter() {
            match op.operator.as_ref() {
                "TJ" => {
                    // Text show
                    if op.operands.len() > 0 {
                        //transaction_date = op.operands[0];
                        let a = &op.operands[0];
                        match a {
                            Primitive::Array(c) => {
                                // If string is "Dividend"
                                if let Primitive::String(actual_string) = &c[0] {
                                    match state {
                                        ParserState::SearchingDividendEntry => {
                                            let rust_string =
                                                actual_string.clone().into_string().unwrap();
                                            if rust_string == "Dividend" {
                                                state = ParserState::SearchingINTCEntry;
                                            } else {
                                                transaction_date = rust_string;
                                            }
                                        }
                                        ParserState::SearchingINTCEntry => {
                                            let rust_string =
                                                actual_string.clone().into_string().unwrap();
                                            if rust_string == "INTC" {
                                                state = ParserState::SearchingTaxEntry;
                                            }
                                        }
                                        ParserState::SearchingTaxEntry => {
                                            tax_us = actual_string
                                                .clone()
                                                .into_string()
                                                .unwrap()
                                                .parse::<f32>()
                                                .unwrap();
                                            state = ParserState::SearchingGrossEntry
                                        }
                                        ParserState::SearchingGrossEntry => {
                                            let gross_us = actual_string
                                                .clone()
                                                .into_string()
                                                .unwrap()
                                                .parse::<f32>()
                                                .unwrap();
                                            state = ParserState::SearchingDividendEntry;
                                            return Ok((transaction_date, gross_us, tax_us));
                                        }
                                    }
                                }
                            }
                            _ => (),
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Err(format!("Error parsing pdf: {}", pdftoparse))
}

fn compute_tax(transactions: Vec<Transaction>) {
    // Gross income from dividends in PLN
    let gross_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate * x.gross_us)
        .sum();
    // Tax paind in US in PLN
    let tax_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate * x.tax_us)
        .sum();
    // Expected full TAX in Poland
    let full_tax_pl = gross_us_pl * 19.0 / 100.0;
    let tax_diff_to_pay_pl = full_tax_pl - tax_us_pl;
    println!("===> PRZYCHOD Z ZAGRANICY: {}", gross_us_pl);
    println!("===> PODATEK ZAPLACONY ZAGRANICA: {}", tax_us_pl);
    println!("DOPLATA: {}", tax_diff_to_pay_pl);
}

fn main() {
    init_logging_infrastructure();

    let mut transactions: Vec<Transaction> = Vec::new();
    let args: Vec<String> = std::env::args().collect();
    // First arg is binary name so advance to actual pdf file names
    let pdfnames = &args[1..];

    log::info!("{:?}", pdfnames);
    log::info!("Started e-trade-tax-helper");
    // Start from second one
    for pdfname in pdfnames {
        // 1. Get PDF parsed and attach exchange rate
        let p = parse_brokerage_statement(&pdfname);

        if let Ok((transaction_date, gross_us, tax_us)) = p {
            let msg = format!(
                "TRANSACTION date: {}, gross: {}, tax_us: {}",
                transaction_date, gross_us, tax_us
            )
            .to_owned();
            println!("{}", msg);
            log::info!("{}", msg);
            let (exchange_rate_date, exchange_rate) =
                get_exchange_rate(&transaction_date).expect("Error getting exchange rate");
            transactions.push(Transaction {
                transaction_date,
                gross_us,
                tax_us,
                exchange_rate_date,
                exchange_rate,
            });
        }
    }
    compute_tax(transactions);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exchange_rate() -> Result<(), String> {
        assert_eq!(
            get_exchange_rate("03/01/21"),
            Ok(("03/01/21".to_string(), 3.7572))
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_parse_brokerage_statement() -> Result<(), String> {
        assert_eq!(
            parse_brokerage_statement("data/example.pdf"),
            Ok(("03/01/21".to_owned(), 574.42, 86.16))
        );
        assert_eq!(
            parse_brokerage_statement("data/example2.pdf"),
            Err(format!("Error parsing pdf: data/example2.pdf"))
        );

        Ok(())
    }
}

// TODO: proxy
// TODO: uts even more
