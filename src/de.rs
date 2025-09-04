// SPDX-FileCopyrightText: 2022-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

use regex::Regex;

pub struct DE {}

impl etradeTaxReturnHelper::Residency for DE {
    fn get_exchange_rates(
        &self,
        dates: &mut std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, f32)>,
        >,
    ) -> Result<(), String> {
        self.get_currency_exchange_rates(dates, "EUR")
    }

    fn parse_exchange_rates(&self, body: &str) -> Result<(f32, String), String> {
        // to find examplery "1 US Dollar = 0.82831 Euros on 2/26/2021</td>"
        let pattern = "1 USD</span> =";
        let start_offset = body
            .find(pattern)
            .ok_or(&format!("Error finding pattern: {}", pattern))?;
        let pattern_slice = &body[start_offset..start_offset + 100]; // 100 characters should be enough
                                                                     // Extract exchange rate (fp32 value)
        log::info!("Exchange rate slice:  {}", pattern_slice);
        let re = Regex::new(r"[0-9]+[.][0-9]+").unwrap();

        let exchange_rate: f32 = match re.find(pattern_slice) {
            Some(hit) => hit.as_str().parse::<f32>().unwrap(),
            None => panic!(),
        };

        // Parse date
        let pattern = "USD to EUR on ";
        let start_date_offset = body
            .find(pattern)
            .ok_or(&format!("Error finding pattern: {}", pattern))?;
        // ..USD to EUR on 2023-2-20....
        let date_pattern_slice = &body[start_date_offset + pattern.chars().count()..];

        let re = Regex::new(r"[0-9]+[-][0-9]+-[0-9]+").unwrap();
        let date_string: &str = match re.find(date_pattern_slice) {
            Some(hit) => hit.as_str(),
            None => panic!(),
        };

        let exchange_rate_date =
            chrono::NaiveDate::parse_from_str(date_string, "%Y-%m-%d").unwrap();

        Ok((
            exchange_rate,
            format!("{}", exchange_rate_date.format("%Y-%m-%d")),
        ))
    }

    fn present_result(
        &self,
        gross_div: f32,
        tax_div: f32,
        gross_sold: f32,
        cost_sold: f32,
    ) -> (Vec<String>, Option<String>) {
        let mut presentation: Vec<String> = vec![];
        presentation.push(format!("===> (DIVIDENDS) INCOME: {:.2} EUR", gross_div));
        presentation.push(format!("===> (DIVIDENDS) TAX PAID: {:.2} EUR", tax_div));
        presentation.push(format!("===> (SOLD STOCK) INCOME: {:.2} EUR", gross_sold));
        presentation.push(format!(
            "===> (SOLD STOCK) TAX DEDUCTIBLE COST: {:.2} EUR",
            cost_sold
        ));
        (presentation, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use etradeTaxReturnHelper::Residency;

    #[test]
    fn test_present_result_de() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(DE {});

        let gross_div = 100.0f32;
        let tax_div = 15.0f32;
        let gross_sold = 1000.0f32;
        let cost_sold = 10.0f32;

        let ref_results: Vec<String> = vec![
            "===> (DIVIDENDS) INCOME: 100.00 EUR".to_string(),
            "===> (DIVIDENDS) TAX PAID: 15.00 EUR".to_string(),
            "===> (SOLD STOCK) INCOME: 1000.00 EUR".to_string(),
            "===> (SOLD STOCK) TAX DEDUCTIBLE COST: 10.00 EUR".to_string(),
        ];

        let (results, _) = rd.present_result(gross_div, tax_div, gross_sold, cost_sold);

        results
            .iter()
            .zip(&ref_results)
            .for_each(|(a, b)| assert_eq!(a, b));

        Ok(())
    }

    #[test]
    fn test_get_exchange_rates_eur() -> Result<(), String> {
        let mut dates: std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, f32)>,
        > = std::collections::HashMap::new();
        dates.insert(
            etradeTaxReturnHelper::Exchange::USD("07/14/23".to_owned()),
            None,
        );

        let rd: DE = DE {};
        rd.get_currency_exchange_rates(&mut dates,"EUR").map_err(|x| "Error: unable to get exchange rates.  Please check your internet connection or proxy settings\n\nDetails:".to_string()+x.as_str())?;

        let mut expected_result: std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, f32)>,
        > = std::collections::HashMap::new();
        expected_result.insert(
            etradeTaxReturnHelper::Exchange::USD("07/14/23".to_owned()),
            Some(("2023-07-13".to_owned(), 0.89429444)),
        );

        assert_eq!(dates, expected_result);

        Ok(())
    }
}
