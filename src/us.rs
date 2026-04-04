// SPDX-FileCopyrightText: 2022-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

use rust_decimal::Decimal;

pub struct US {}
impl etradeTaxReturnHelper::Residency for US {
    fn get_exchange_rates(
        &self,
        dates: &mut std::collections::HashMap<
            etradeTaxReturnHelper::Exchange,
            Option<(String, Decimal)>,
        >,
    ) -> Result<(), String> {
        dates.iter_mut().for_each(|(_date, val)| {
            *val = Some(("N/A".to_owned(), Decimal::ONE));
        });
        Ok(())
    }

    fn present_result(
        &self,
        gross_interests: Decimal,
        gross_div: Decimal,
        tax_div: Decimal,
        gross_sold: Decimal,
        cost_sold: Decimal,
    ) -> (Vec<String>, Option<String>) {
        let total_gross_div = gross_interests + gross_div;
        let mut presentation: Vec<String> = vec![];
        presentation.push(format!(
            "===> (DIVIDENDS+INTERESTS) INCOME: ${:.2}",
            total_gross_div
        ));
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
    use rust_decimal::dec;
    #[test]
    fn test_present_result_us() -> Result<(), String> {
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(US {});

        let gross_div = dec!(100.0);
        let tax_div = dec!(15.0);
        let gross_sold = dec!(1000.0);
        let cost_sold = dec!(10.0);

        let ref_results: Vec<String> = vec![
            "===> (DIVIDENDS+INTERESTS) INCOME: $100.00".to_string(),
            "===> (DIVIDENDS) TAX PAID: $15.00".to_string(),
            "===> (SOLD STOCK) INCOME: $1000.00".to_string(),
            "===> (SOLD STOCK) TAX DEDUCTIBLE COST: $10.00".to_string(),
        ];

        let (results, _) = rd.present_result(dec!(0.0), gross_div, tax_div, gross_sold, cost_sold);

        results
            .iter()
            .zip(&ref_results)
            .for_each(|(a, b)| assert_eq!(a, b));

        Ok(())
    }
}
