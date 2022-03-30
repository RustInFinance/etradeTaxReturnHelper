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


// TODO(jczaja) : Parse JSON response

fn GetExchangeRate(transaction_date: &str) {
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
    let converted_date = chrono::NaiveDate::parse_from_str(transaction_date, "%m/%d/%y").unwrap();
    let exchange_rate_url: String = base_exchange_rate_url.to_string()
        + &format!("{}", converted_date.format("%Y-%m-%d"))
        + "/?format=json";

    // checked_sub_signed(self, rhs: OldDuration) -> Option<NaiveDate>

    let body = client
        .get(&(exchange_rate_url))
        .send();
    let mut actual_body = body.expect(&format!("Getting Exchange Rate from NBP ({}) failed", exchange_rate_url));
    if actual_body.status().is_success() == false {
        panic!();
    } else {
        println!("RESPONSE {:#?}", actual_body);

        let exchange_rate_data = actual_body.json::<NBPResponse<ExchangeRate>>().expect("Error converting response to JSON");
        println!("body of exchange_rate = {:#?}", exchange_rate_data);

    }
}

fn parse_brokerage_statement(pdftoparse: &str) -> Result<(String, f32, f32), String> {
    //2. parsing each pdf
    let mypdffile = File::<Vec<u8>>::open(pdftoparse).unwrap();

    let mut state = ParserState::SearchingDividendEntry;
    let mut transaction_date: String = "N/A".to_string();
    let mut tax_us = 0.0;
    let mut gross_us = 0.0;

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
                                            gross_us = actual_string
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

fn main() {
    // 1. Get PDF parsed and attach exchange rate
    let p = parse_brokerage_statement("data/example.pdf");
    let (transaction_date, gross_us, tax_us) = match p {
        Ok(t) => t,
        Err(msg) => panic!("{}", msg),
    };
    println!(
        "TRANSACTION date: {}, gross: {}, tax_us: {}",
        transaction_date, gross_us, tax_us
    );
    GetExchangeRate(&transaction_date);
}

//TODO(jczaja): UT
//TODO(jczaja): UT get exchange rate with fixed date
