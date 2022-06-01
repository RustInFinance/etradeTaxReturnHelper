mod logging;
mod pdfparser;
mod transactions;
mod xlsxparser;

use chrono;

type ReqwestClient = reqwest::blocking::Client;

pub use logging::ResultExt;
use transactions::{
    create_detailed_div_transactions, create_detailed_sold_transactions,
    reconstruct_sold_transactions, verify_dividends_transactions,
};

#[derive(Debug, PartialEq, PartialOrd)]
pub struct Transaction {
    pub transaction_date: String,
    pub gross_us: f32,
    pub tax_us: f32,
    pub exchange_rate_date: String,
    pub exchange_rate: f32,
}

// 1. settlement date
// 2. date of purchase
// 3. net income
// 4. cost cost basis
#[derive(Debug, PartialEq, PartialOrd)]
pub struct SoldTransaction {
    pub settlement_date: String,
    pub acquisition_date: String,
    pub income_us: f32,
    pub cost_basis: f32,
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

fn compute_div_taxation(transactions: Vec<Transaction>) -> (f32, f32) {
    // Gross income from dividends in target currency (PLN, EUR etc.)
    let gross_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate * x.gross_us)
        .sum();
    // Tax paid in US in PLN
    let tax_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate * x.tax_us)
        .sum();
    (gross_us_pl, tax_us_pl)
}

fn compute_sold_taxation(transactions: Vec<SoldTransaction>) -> (f32, f32) {
    // Net income from sold stock in target currency (PLN, EUR etc.)
    let gross_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate_settlement * x.income_us)
        .sum();
    // Cost of income e.g. cost_basis[target currency]
    let cost_us_pl: f32 = transactions
        .iter()
        .map(|x| x.exchange_rate_acquisition * x.cost_basis)
        .sum();
    (gross_us_pl, cost_us_pl)
}

pub fn run_taxation(
    rd: &Box<dyn Residency>,
    names: clap::Values,
) -> Result<(f32, f32, f32, f32), String> {
    let mut parsed_div_transactions: Vec<(String, f32, f32)> = vec![];
    let mut parsed_sold_transactions: Vec<(String, String, i32, f32, f32)> = vec![];
    let mut parsed_gain_and_losses: Vec<(String, String, f32, f32, f32)> = vec![];

    // 1. Parse PDF and XLSX documents to get list of transactions
    names.for_each(|x| {
        // If name contains .pdf then parse as pdf
        // if name contains .xlsx then parse as spreadsheet
        if x.contains(".pdf") {
            let (mut div_t, mut sold_t, _) = pdfparser::parse_brokerage_statement(x);
            parsed_div_transactions.append(&mut div_t);
            parsed_sold_transactions.append(&mut sold_t);
        } else {
            parsed_gain_and_losses.append(&mut xlsxparser::parse_gains_and_losses(x));
        }
    });
    // 2. Verify Transactions
    match verify_dividends_transactions(&parsed_div_transactions) {
        Ok(()) => log::info!("Dividends transactions are consistent"),
        Err(msg) => {
            println!("{}", msg);
            log::warn!("{}", msg);
        }
    }

    // 3. Verify and create full sold transactions info needed for TAX purposes
    let detailed_sold_transactions =
        reconstruct_sold_transactions(&parsed_sold_transactions, &parsed_gain_and_losses)
            .expect_and_log("Error reconstructing detailed sold transactions.");

    // 4. Get Exchange rates
    // Gather all trade , settlement and transaction dates into hash map to be passed to
    // get_exchange_rate
    // Hash map : Key(event date) -> (preceeding date, exchange_rate)
    let mut dates: std::collections::HashMap<String, Option<(String, f32)>> =
        std::collections::HashMap::new();
    parsed_div_transactions
        .iter()
        .for_each(|(trade_date, _, _)| {
            if dates.contains_key(trade_date) == false {
                dates.insert(trade_date.clone(), None);
            }
        });
    detailed_sold_transactions.iter().for_each(
        |(trade_date, settlement_date, acquisition_date, _, _)| {
            if dates.contains_key(trade_date) == false {
                dates.insert(trade_date.clone(), None);
            }
            if dates.contains_key(settlement_date) == false {
                dates.insert(settlement_date.clone(), None);
            }
            if dates.contains_key(acquisition_date) == false {
                dates.insert(acquisition_date.clone(), None);
            }
        },
    );

    rd.get_exchange_rates(&mut dates)
        .expect_and_log("Error: unable to get exchange rates");

    // Make a detailed_div_transactions
    let transactions = create_detailed_div_transactions(parsed_div_transactions, &dates);
    let sold_transactions = create_detailed_sold_transactions(detailed_sold_transactions, &dates);

    let (gross_div, tax_div) = compute_div_taxation(transactions);
    let (gross_sold, cost_sold) = compute_sold_taxation(sold_transactions);
    Ok((gross_div, tax_div, gross_sold, cost_sold))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_simple_div_taxation() -> Result<(), String> {
        // Init Transactions
        let transactions: Vec<Transaction> = vec![Transaction {
            transaction_date: "N/A".to_string(),
            gross_us: 100.0,
            tax_us: 25.0,
            exchange_rate_date: "N/A".to_string(),
            exchange_rate: 4.0,
        }];
        assert_eq!(compute_div_taxation(transactions), (400.0, 100.0));
        Ok(())
    }

    #[test]
    fn test_div_taxation() -> Result<(), String> {
        // Init Transactions
        let transactions: Vec<Transaction> = vec![
            Transaction {
                transaction_date: "N/A".to_string(),
                gross_us: 100.0,
                tax_us: 25.0,
                exchange_rate_date: "N/A".to_string(),
                exchange_rate: 4.0,
            },
            Transaction {
                transaction_date: "N/A".to_string(),
                gross_us: 126.0,
                tax_us: 10.0,
                exchange_rate_date: "N/A".to_string(),
                exchange_rate: 3.5,
            },
        ];
        assert_eq!(
            compute_div_taxation(transactions),
            (400.0 + 126.0 * 3.5, 100.0 + 10.0 * 3.5)
        );
        Ok(())
    }

    #[test]
    fn test_simple_sold_taxation() -> Result<(), String> {
        // Init Transactions
        let transactions: Vec<SoldTransaction> = vec![SoldTransaction {
            settlement_date: "N/A".to_string(),
            acquisition_date: "N/A".to_string(),
            income_us: 100.0,
            cost_basis: 70.0,
            exchange_rate_settlement_date: "N/A".to_string(),
            exchange_rate_settlement: 5.0,
            exchange_rate_acquisition_date: "N/A".to_string(),
            exchange_rate_acquisition: 6.0,
        }];
        assert_eq!(
            compute_sold_taxation(transactions),
            (100.0 * 5.0, 70.0 * 6.0)
        );
        Ok(())
    }

    #[test]
    fn test_sold_taxation() -> Result<(), String> {
        // Init Transactions
        let transactions: Vec<SoldTransaction> = vec![
            SoldTransaction {
                settlement_date: "N/A".to_string(),
                acquisition_date: "N/A".to_string(),
                income_us: 100.0,
                cost_basis: 70.0,
                exchange_rate_settlement_date: "N/A".to_string(),
                exchange_rate_settlement: 5.0,
                exchange_rate_acquisition_date: "N/A".to_string(),
                exchange_rate_acquisition: 6.0,
            },
            SoldTransaction {
                settlement_date: "N/A".to_string(),
                acquisition_date: "N/A".to_string(),
                income_us: 10.0,
                cost_basis: 4.0,
                exchange_rate_settlement_date: "N/A".to_string(),
                exchange_rate_settlement: 2.0,
                exchange_rate_acquisition_date: "N/A".to_string(),
                exchange_rate_acquisition: 3.0,
            },
        ];
        assert_eq!(
            compute_sold_taxation(transactions),
            (100.0 * 5.0 + 10.0 * 2.0, 70.0 * 6.0 + 4.0 * 3.0)
        );
        Ok(())
    }
}
