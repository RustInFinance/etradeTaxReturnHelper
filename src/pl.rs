pub struct PL {}

impl etradeTaxReturnHelper::Residency for PL {
    fn get_exchange_rate(&self, transaction_date: &str) -> Result<(String, f32), String> {
        self.get_nbp_exchange_rate_to_pln(transaction_date, "usd")
    }

    fn present_result(&self, gross_us_pl: f32, tax_us_pl: f32) {
        println!("===> PRZYCHOD Z ZAGRANICY: {} PLN", gross_us_pl);
        println!("===> PODATEK ZAPLACONY ZAGRANICA: {} PLN", tax_us_pl);
        // Expected full TAX in Poland
        let full_tax_pl = gross_us_pl * 19.0 / 100.0;
        // Normally you pay 15% in US, but if you made wrong
        // choices in your residency application you may be charged 30%
        // in that case you do not pay anything in Poland because you paid
        // 30% alrady in US
        let tax_diff_to_pay_pl = if full_tax_pl > tax_us_pl {
            full_tax_pl - tax_us_pl
        } else {
            0.0
        };
        println!("DOPLATA: {} PLN", tax_diff_to_pay_pl);
    }
}
