mod logging;

use chrono;
use logging::ResultExt;
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

    fn get_nbp_exchange_rate_to_pln(
        &self,
        transaction_date: &str,
        currency_code: &str,
    ) -> Result<(String, f32), String> {
        // proxies are taken from env vars: http_proxy and https_proxy
        let http_proxy = std::env::var("http_proxy");
        let https_proxy = std::env::var("https_proxy");

        // If there is proxy then pick first URL
        let base_client = ReqwestClient::builder();
        let client = match &http_proxy {
            Ok(proxy) => base_client
                .proxy(reqwest::Proxy::http(proxy).expect_and_log("Error setting HTTP proxy")),
            Err(_) => base_client,
        };
        let client = match &https_proxy {
            Ok(proxy) => client
                .proxy(reqwest::Proxy::https(proxy).expect_and_log("Error setting HTTP proxy")),
            Err(_) => client,
        };
        let client = client
            .build()
            .expect_and_log("Could not create REST API client");

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
                .expect_and_log("Error traversing date");

            let exchange_rate_url: String = base_exchange_rate_url.to_string()
                + &format!("{}/{}", currency_code, converted_date.format("%Y-%m-%d"))
                + "/?format=json";

            let body = client.get(&(exchange_rate_url)).send();
            let actual_body = body.expect_and_log(&format!(
                "Getting Exchange Rate from NBP ({}) failed",
                exchange_rate_url
            ));
            is_success = actual_body.status().is_success();
            if is_success == true {
                log::info!("RESPONSE {:#?}", actual_body);

                let nbp_response = actual_body
                    .json::<NBPResponse<ExchangeRate>>()
                    .expect_and_log("Error converting response to JSON");
                log::info!("body of exchange_rate = {:#?}", nbp_response);
                exchange_rate = nbp_response.rates[0].mid;
                exchange_rate_date = format!("{}", converted_date.format("%Y-%m-%d"));
            }
        }

        Ok((exchange_rate_date, exchange_rate))
    }

    // Default parser (not to be used)
    fn parse_exchange_rates(&self, _body: &str) -> Result<(f32, String), String> {
        panic!("This method should not be used. Implement your own if needed!");
    }

    fn get_exchange_rates(
        &self,
        transaction_date: &str,
        from: &str,
        to: &str,
    ) -> Result<(String, f32), String> {
        // proxies are taken from env vars: http_proxy and https_proxy
        let http_proxy = std::env::var("http_proxy");
        let https_proxy = std::env::var("https_proxy");

        // If there is proxy then pick first URL
        let base_client = ReqwestClient::builder();
        let client = match &http_proxy {
            Ok(proxy) => base_client
                .proxy(reqwest::Proxy::http(proxy).expect_and_log("Error setting HTTP proxy")),
            Err(_) => base_client,
        };
        let client = match &https_proxy {
            Ok(proxy) => client
                .proxy(reqwest::Proxy::https(proxy).expect_and_log("Error setting HTTP proxy")),
            Err(_) => client,
        };
        let client = client.build().expect_and_log("Could not create client");

        // Example URL: https://www.exchange-rates.org/Rate/USD/EUR/2-27-2021

        let base_exchange_rate_url = "https://www.exchange-rates.org/Rate/";
        let mut converted_date =
            chrono::NaiveDate::parse_from_str(transaction_date, "%m/%d/%y").unwrap();

        converted_date = converted_date
            .checked_sub_signed(chrono::Duration::days(1))
            .expect_and_log("Error traversing date");

        let exchange_rate_url: String = base_exchange_rate_url.to_string()
            + &format!("{}/{}/{}", from, to, converted_date.format("%m-%d-%Y"))
            + "/?format=json";

        let body = client.get(&(exchange_rate_url)).send();
        let actual_body = body.expect_and_log(&format!(
            "Getting Exchange Rate from Exchange-Rates.org ({}) failed",
            exchange_rate_url
        ));
        if actual_body.status().is_success() {
            log::info!("RESPONSE {:#?}", actual_body);

            let exchange_rates_response = actual_body
                .text()
                .expect_and_log("Error converting response to Text");
            log::info!("body of exchange_rate = {:#?}", exchange_rates_response);
            // parsing text response
            if let Ok((exchange_rate, exchange_rate_date)) =
                self.parse_exchange_rates(&exchange_rates_response)
            {
                return Ok((exchange_rate_date, exchange_rate));
            }
        }

        Err("Error getting exchange rate".to_owned())
    }
}
