pub struct US {}
impl etradeTaxReturnHelper::Residency for US {
    fn get_exchange_rates(
        &self,
        dates: &mut std::collections::HashMap<String, Option<(String, f32)>>,
    ) -> Result<(), String> {
        dates.iter_mut().for_each(|(date, val)| {
            *val = Some(("N/A".to_owned(), 1.0));
        });
        Ok(())
    }

    fn present_result(&self, gross_div: f32, tax_div: f32, gross_sold: f32, cost_sold: f32) {
        println!("===> (DIVIDENDS) INCOME: ${} ", gross_div);
        println!("===> (DIVIDENDS) TAX PAID: ${}", tax_div);
        println!("===> (SOLD STOCK) INCOME: ${} ", gross_sold);
        println!("===> (SOLD STOCK) TAX PAID: ${}", cost_sold);
    }
}
