use std::fmt;

// Let's extend Result with logging
pub trait ResultExt<T> {
    fn expect_and_log(self, msg: &str) -> T;
}

impl<T, E: fmt::Debug> ResultExt<T> for Result<T, E> {
    fn expect_and_log(self, err_msg: &str) -> T {
        self.map_err(|e| {
            log::error!("{}", err_msg);
            e
        })
        .expect(err_msg)
    }
}

impl<T> ResultExt<T> for Option<T> {
    fn expect_and_log(self, err_msg: &str) -> T {
        self.or_else(|| {
            log::error!("{}", err_msg);
            None
        })
        .expect(err_msg)
    }
}

#[allow(dead_code)]
pub fn init_logging_infrastructure() {
    // Make a default logging level: error
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "error")
    }
    simple_logger::SimpleLogger::new().env().init().unwrap();
}

