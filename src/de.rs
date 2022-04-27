pub struct DE {}

impl etradeTaxReturnHelper::Residency for DE {
    fn get_exchange_rate(&self, transaction_date: &str) -> Result<(String, f32), String> {
        // To get USD to EUR echange rate we will get USD to PLN and then EUR to PLN and
        // then compute USD/EUR e.g. USD/EUR = USD/PLN *  1/(EUR/PLN)
        let usd_pln_res = self.get_nbp_exchange_rate_to_pln(transaction_date, "usd");
        let eur_pln_res = self.get_nbp_exchange_rate_to_pln(transaction_date, "eur");
        // If any of exchage rates is an error then declare failure
        if let Ok((_, usd_pln)) = usd_pln_res {
            log::info!("USD/PLN : {}", usd_pln);
            if let Ok((date, eur_pln)) = eur_pln_res {
                log::info!("EUR/PLN : {}", eur_pln);
                let usd_eur: f32 = usd_pln / eur_pln;
                log::info!("USD/EUR : {}", usd_eur);
                return Ok((date, usd_eur));
            } else {
                let msg = "Error: unable to get EUR/PLN exchange rate";
                log::error!("{}", msg);
                return Err(msg.to_owned());
            }
        } else {
            let msg = "Error: unable to get USD/PLN exchange rate";
            log::error!("{}", msg);
            return Err(msg.to_owned());
        };
    }

    fn present_result(&self, gross_us_de: f32, tax_us_de: f32) {
        println!("===> GROSS INCOME: {} EUR", gross_us_de);
        println!("===> TAX PAID IN US: {} EUR", tax_us_de);
        // Expected full TAX in Poland
        let full_tax_de = gross_us_de * 25.0 / 100.0;
        // Normally you pay 15% in US, but if you made wrong
        // choices in your residency application you may be charged 30%
        // in that case you do not pay anything in Poland because you paid
        // 30% alrady in US
        let tax_diff_to_pay_de = if full_tax_de > tax_us_de {
            full_tax_de - tax_us_de
        } else {
            0.0
        };
        println!("ADDITIONAL TAX TO BE PAID: {} EUR", tax_diff_to_pay_de);
    }
}
