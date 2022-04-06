pub struct US {}
impl etradeTaxReturnHelper::Residency for US {
    fn get_exchange_rate(&self, transaction_date: &str) -> Result<(String, f32), String> {
        Ok(("N/A".to_owned(), 1.0))
    }

    fn present_result(&self, gross: f32, tax: f32) {
        println!("===> INCOME: ${} ", gross);
        println!("===> TAX PAID: ${}", tax);
    }
}
