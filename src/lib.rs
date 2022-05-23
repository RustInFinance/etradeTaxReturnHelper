mod logging;

use chrono;

type ReqwestClient = reqwest::blocking::Client;

pub use logging::ResultExt;

pub struct Transaction {
    pub transaction_date: String,
    pub gross_us: f32,
    pub tax_us: f32,
    pub exchange_rate_date: String,
    pub exchange_rate: f32,
}

// 1. trade date
// 2. settlement date
// 3. date of purchase
// 4. gross income
// 5. fee+commission
// 6. cost cost basis
pub struct Sold_Transaction {
    pub trade_date: String,
    pub settlement_date: String,
    pub acquisition_date: String,
    pub gross_us: f32,
    pub total_fee: f32,
    pub cost_basis: f32,
    pub exchange_rate_trade_date: String,
    pub exchange_rate_trade: f32,
    pub exchange_rate_settlement_date: String,
    pub exchange_rate_settlement: f32,
    pub exchange_rate_acquisition_date: String,
    pub exchange_rate_acquisition: f32,
}

pub trait Residency {
    //    fn get_exchange_rate(&self, transaction_date: &str) -> Result<(String, f32), String>;
    fn present_result(&self, gross_div: f32, tax_div: f32, gross_sold: f32, cost_sold: f32);
    fn get_exchange_rates(
        &self,
        dates: &mut std::collections::HashMap<String, Option<(String, f32)>>,
    ) -> Result<(), String>;

    // Default parser (not to be used)
    fn parse_exchange_rates(&self, _body: &str) -> Result<(f32, String), String> {
        panic!("This method should not be used. Implement your own if needed!");
    }

    fn get_currency_exchange_rates(
        &self,
        dates: &mut std::collections::HashMap<String, Option<(String, f32)>>,
        from: &str,
        to: &str,
    ) -> Result<(), String> {
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

        dates.iter_mut().for_each(|(date, val)| {
            let mut converted_date = chrono::NaiveDate::parse_from_str(&date, "%m/%d/%y").unwrap();

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
                log::info!("body of exchange_rate = {:#?}", &exchange_rates_response);
                // parsing text response
                if let Ok((exchange_rate, exchange_rate_date)) =
                    self.parse_exchange_rates(&exchange_rates_response)
                {
                    *val = Some((exchange_rate_date, exchange_rate));
                }
            } else {
                panic!("Error getting exchange rate");
            }
        });

        Ok(())
    }
}
