pub struct US {}
impl etradeTaxReturnHelper::Residency for US {
    fn get_exchange_rates(
        &self,
        dates: &mut std::collections::HashMap<String, Option<(String, f32)>>
    ) -> Result<(), String>{
        dates.iter_mut().for_each(|(date, val)| {
                    *val = Some(("N/A".to_owned(), 1.0));
            });
        Ok(())
    }

    fn present_result(&self, gross: f32, tax: f32) {
        println!("===> INCOME: ${} ", gross);
        println!("===> TAX PAID: ${}", tax);
    }
}
