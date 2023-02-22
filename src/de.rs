use regex::Regex;

pub struct DE {}

impl etradeTaxReturnHelper::Residency for DE {
    fn get_exchange_rates(
        &self,
        dates: &mut std::collections::HashMap<String, Option<(String, f32)>>,
    ) -> Result<(), String> {
        self.get_currency_exchange_rates(dates, "USD", "EUR")
    }

    fn parse_exchange_rates(&self, body: &str) -> Result<(f32, String), String> {
        // to find examplery "1 US Dollar = 0.82831 Euros on 2/26/2021</td>"
        let pattern = "1 USD</span> =";
        let start_offset = body
            .find(pattern)
            .ok_or(&format!("Error finding pattern: {}", pattern))?;
        let pattern_slice = &body[start_offset..start_offset + 100]; // 100 characters should be enough
                                                                     // Extract exchange rate (fp32 value)
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

    fn present_result(&self, gross_div: f32, tax_div: f32, gross_sold: f32, cost_sold: f32) {
        println!("===> (DIVIDENDS) INCOME: {} EUR", gross_div);
        println!("===> (DIVIDENDS) TAX PAID: {} EUR", tax_div);
        println!("===> (SOLD STOCK) INCOME: {} EUR", gross_sold);
        println!("===> (SOLD STOCK) TAX DEDUCTIBLE COST: {} EUR", cost_sold);
    }
}
