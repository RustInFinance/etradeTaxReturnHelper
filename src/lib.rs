use chrono;
use serde::{Deserialize, Serialize};

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


pub trait Residency {
    fn get_exchange_rate(&self, transaction_date: &str) -> Result<(String, f32), String>;
    fn present_result(&self, gross: f32, tax: f32);

    fn get_nbp_exchange_rate_to_pln(&self, transaction_date: &str, currency_code : &str) -> Result<(String, f32), String> {
        // proxies are taken from env vars: http_proxy and https_proxy
        let http_proxy = std::env::var("http_proxy");
        let https_proxy = std::env::var("https_proxy");

        // If there is proxy then pick first URL
        let base_client = ReqwestClient::builder();
        let client = match &http_proxy {
            Ok(proxy) => {
                base_client.proxy(reqwest::Proxy::http(proxy).expect("Error setting HTTP proxy"))
            }
            Err(_) => base_client,
        };
        let client = match &https_proxy {
            Ok(proxy) => {
                client.proxy(reqwest::Proxy::https(proxy).expect("Error setting HTTP proxy"))
            }
            Err(_) => client,
        };
        let client = client.build().expect("Could not create REST API client");

        let base_exchange_rate_url = "http://api.nbp.pl/api/exchangerates/rates/a/";
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
                + &format!("{}/{}",currency_code, converted_date.format("%Y-%m-%d"))
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

}
