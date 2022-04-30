use etradeTaxReturnHelper::Transaction;

pub struct US {}
impl etradeTaxReturnHelper::Residency for US {
    fn get_exchange_rates(
        &self,
        transactions: Vec<(String, f32, f32)>,
    ) -> Result<Vec<Transaction>, String> {
        let mut detailed_transactions: Vec<Transaction> = Vec::new();
        transactions
            .iter()
            .for_each(|(transaction_date, gross_us, tax_us)| {
                detailed_transactions.push(Transaction {
                    transaction_date: transaction_date.clone(),
                    gross_us: gross_us.clone(),
                    tax_us: tax_us.clone(),
                    exchange_rate_date: "N/A".to_owned(),
                    exchange_rate: 1.0,
                })
            });
        Ok(detailed_transactions)
    }

    fn present_result(&self, gross: f32, tax: f32) {
        println!("===> INCOME: ${} ", gross);
        println!("===> TAX PAID: ${}", tax);
    }
}
