use serde::{Deserialize, Serialize};

pub use crate::logging::ResultExt;

pub struct PL {}

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
#[allow(non_snake_case)]
struct ExchangeRate {
    no: String,
    effectiveDate: String,
    mid: f32,
}

impl etradeTaxReturnHelper::Residency for PL {
    fn get_exchange_rates(
        &self,
        dates: &mut std::collections::HashMap<String, Option<(String, f32)>>,
    ) -> Result<(), String> {
        // proxies are taken from env vars: http_proxy and https_proxy
        let http_proxy = std::env::var("http_proxy");
        let https_proxy = std::env::var("https_proxy");

        // If there is proxy then pick first URL
        let base_client = ReqwestClient::builder();
        let client = match &http_proxy {
            Ok(proxy) => base_client.proxy(
                reqwest::Proxy::http(proxy)
                    .map_err(|x| format!("Error setting HTTP proxy. \nDetails: {}", x))?,
            ),
            Err(_) => base_client,
        };
        let client = match &https_proxy {
            Ok(proxy) => client.proxy(
                reqwest::Proxy::https(proxy)
                    .map_err(|x| format!("Error setting HTTPS proxy. \nDetails: {}", x))?,
            ),
            Err(_) => client,
        };
        let client = client
            .build()
            .map_err(|_| "Could not create REST API client")?;

        let base_exchange_rate_url = "https://api.nbp.pl/api/exchangerates/rates/a/";

        dates.iter_mut().try_for_each(|(date, val)| {
            let mut converted_date = chrono::NaiveDate::parse_from_str(&date, "%m/%d/%y").unwrap();

            // Try to get exchange rate going backwards with dates till success
            let mut is_success = false;
            while is_success == false {
                converted_date = converted_date
                    .checked_sub_signed(chrono::Duration::days(1))
                    .ok_or("Error traversing date")?;

                let exchange_rate_url: String = base_exchange_rate_url.to_string()
                    + format!("usd/{}", converted_date.format("%Y-%m-%d")).as_str()
                    + "/?format=json";

                let body = client.get(&(exchange_rate_url)).send();
                let actual_body = body.map_err(|_| {
                    format!(
                        "Getting Exchange Rate from NBP ({}) failed",
                        exchange_rate_url
                    )
                })?;
                is_success = actual_body.status().is_success();
                if is_success == true {
                    log::info!("RESPONSE {:#?}", actual_body);

                    let nbp_response = actual_body
                        .json::<NBPResponse<ExchangeRate>>()
                        .map_err(|_| "Error: getting exchange rate from NBP")?;
                    log::info!("body of exchange_rate = {:#?}", nbp_response);
                    let exchange_rate = nbp_response.rates[0].mid;
                    let exchange_rate_date = format!("{}", converted_date.format("%Y-%m-%d"));
                    *val = Some((exchange_rate_date, exchange_rate));
                };
            }
            Ok::<(), String>(())
        })?;
        Ok(())
    }

    fn present_result(
        &self,
        gross_div: f32,
        tax_div: f32,
        gross_sold: f32,
        cost_sold: f32,
    ) -> (Vec<String>, Option<String>) {
        let mut presentation: Vec<String> = vec![];
        let tax_pl = 0.19 * gross_div;
        presentation.push(format!(
            "(DYWIDENDY) PRZYCHOD Z ZAGRANICY: {:.2} PLN",
            gross_div
        ));
        presentation.push(format!(
            "===> (DYWIDENDY) ZRYCZALTOWANY PODATEK: {:.2} PLN",
            tax_pl
        ));
        presentation.push(format!(
            "===> (DYWIDENDY) PODATEK ZAPLACONY ZAGRANICA: {:.2} PLN",
            tax_div
        ));
        presentation.push(format!(
            "===> (SPRZEDAZ AKCJI) PRZYCHOD Z ZAGRANICY: {:.2} PLN",
            gross_sold
        ));
        presentation.push(format!(
            "===> (SPRZEDAZ AKCJI) KOSZT UZYSKANIA PRZYCHODU: {:.2} PLN",
            cost_sold
        ));
        if tax_div > tax_pl {
            (presentation,Some(format!("Warning: Tax paid in US({tax_div} PLN) is higher than the tax that you are to pay in Poland({tax_pl} PLN). This usually means that there was a problem with declaration of your residency to avoid double taxation")))
        } else {
            (presentation, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_present_result_pl() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(PL {});

        let gross_div = 100.0f32;
        let tax_div = 15.0f32;
        let gross_sold = 1000.0f32;
        let cost_sold = 10.0f32;

        let ref_results: Vec<String> = vec![
            "(DYWIDENDY) PRZYCHOD Z ZAGRANICY: 100.00 PLN".to_string(),
            "===> (DYWIDENDY) ZRYCZALTOWANY PODATEK: 19.00 PLN".to_string(),
            "===> (DYWIDENDY) PODATEK ZAPLACONY ZAGRANICA: 15.00 PLN".to_string(),
            "===> (SPRZEDAZ AKCJI) PRZYCHOD Z ZAGRANICY: 1000.00 PLN".to_string(),
            "===> (SPRZEDAZ AKCJI) KOSZT UZYSKANIA PRZYCHODU: 10.00 PLN".to_string(),
        ];

        let (results, _) = rd.present_result(gross_div, tax_div, gross_sold, cost_sold);

        results
            .iter()
            .zip(&ref_results)
            .for_each(|(a, b)| assert_eq!(a, b));

        Ok(())
    }

    #[test]
    fn test_present_result_double_taxation_warning_pl() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(PL {});

        let gross_div = 100.0f32;
        let tax_div = 30.0f32;
        let gross_sold = 1000.0f32;
        let cost_sold = 10.0f32;

        let ref_results: Vec<String> = vec![
            "(DYWIDENDY) PRZYCHOD Z ZAGRANICY: 100.00 PLN".to_string(),
            "===> (DYWIDENDY) ZRYCZALTOWANY PODATEK: 19.00 PLN".to_string(),
            "===> (DYWIDENDY) PODATEK ZAPLACONY ZAGRANICA: 30.00 PLN".to_string(),
            "===> (SPRZEDAZ AKCJI) PRZYCHOD Z ZAGRANICY: 1000.00 PLN".to_string(),
            "===> (SPRZEDAZ AKCJI) KOSZT UZYSKANIA PRZYCHODU: 10.00 PLN".to_string(),
        ];

        let (results, warning) = rd.present_result(gross_div, tax_div, gross_sold, cost_sold);

        results
            .iter()
            .zip(&ref_results)
            .for_each(|(a, b)| assert_eq!(a, b));

        let ref_msg = "Warning: Tax paid in US(30 PLN) is higher than the tax that you are to pay in Poland(19 PLN). This usually means that there was a problem with declaration of your residency to avoid double taxation".to_string();

        match (warning) {
            Some(msg) => assert_eq!(msg, ref_msg),
            None => return Err("Error: expected information on to high tax".to_string()),
        }

        Ok(())
    }
}
