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
        let pattern = "1 US Dollar = ";
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
        let pattern = "Euros on ";
        let start_date_offset = pattern_slice
            .find(pattern)
            .ok_or(&format!("Error finding pattern: {}", pattern))?;
        // 2/26/2021 </td>.....
        let end_date_pattern = "</td";
        let date_pattern_slice = &pattern_slice[start_date_offset + pattern.chars().count()..];
        let end_date_offset = date_pattern_slice
            .find(end_date_pattern)
            .ok_or(&format!("Error finding pattern: {}", end_date_pattern))?;

        let date_string = &date_pattern_slice[0..end_date_offset];
        let exchange_rate_date =
            chrono::NaiveDate::parse_from_str(date_string, "%m/%d/%Y").unwrap();

        Ok((
            exchange_rate,
            format!("{}", exchange_rate_date.format("%Y-%m-%d")),
        ))
    }

    fn present_result(&self, gross_div: f32, tax_div: f32, gross_sold: f32, cost_sold: f32) {
        println!("===> (DIVIDENDS) INCOME: {} EUR", gross_div);
        println!("===> (DIVIDENDS) TAX PAID: {} EUR", tax_div);
        println!("===> (SOLD STOCK) INCOME: {} EUR", gross_sold);
        println!("===> (SOLD STOCK) TAX PAID: {} EUR", cost_sold);
    }
}
