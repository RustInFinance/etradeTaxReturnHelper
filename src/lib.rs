pub trait Residency {
    fn get_exchange_rate(&self, transaction_date: &str) -> Result<(String, f32), String>;
    fn present_result(&self, gross: f32, tax: f32);
}
