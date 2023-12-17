pub struct US {}
impl etradeTaxReturnHelper::Residency for US {
    fn get_exchange_rates(
        &self,
        dates: &mut std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, f32)>,
        >,
    ) -> Result<(), String> {
        dates.iter_mut().for_each(|(_date, val)| {
            *val = Some(("N/A".to_owned(), 1.0));
        });
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
        presentation.push(format!("===> (DIVIDENDS) INCOME: ${:.2}", gross_div));
        presentation.push(format!("===> (DIVIDENDS) TAX PAID: ${:.2}", tax_div));
        presentation.push(format!("===> (SOLD STOCK) INCOME: ${:.2}", gross_sold));
        presentation.push(format!(
            "===> (SOLD STOCK) TAX DEDUCTIBLE COST: ${:.2}",
            cost_sold
        ));
        (presentation, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_present_result_us() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(US {});

        let gross_div = 100.0f32;
        let tax_div = 15.0f32;
        let gross_sold = 1000.0f32;
        let cost_sold = 10.0f32;

        let ref_results: Vec<String> = vec![
            "===> (DIVIDENDS) INCOME: $100.00".to_string(),
            "===> (DIVIDENDS) TAX PAID: $15.00".to_string(),
            "===> (SOLD STOCK) INCOME: $1000.00".to_string(),
            "===> (SOLD STOCK) TAX DEDUCTIBLE COST: $10.00".to_string(),
        ];

        let (results, _) = rd.present_result(gross_div, tax_div, gross_sold, cost_sold);

        results
            .iter()
            .zip(&ref_results)
            .for_each(|(a, b)| assert_eq!(a, b));

        Ok(())
    }
}
