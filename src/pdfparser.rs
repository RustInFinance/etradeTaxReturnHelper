use pdf::file::File;
use pdf::primitive::Primitive;

pub use crate::logging::ResultExt;

enum TransactionType {
    Dividends,
    Sold,
    Trade,
}

enum ParserState {
    SearchingTransactionEntry,
    ProcessingTransaction(TransactionType),
}

pub trait Entry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString);
    fn getf32(&self) -> Option<f32> {
        None
    }
    fn geti32(&self) -> Option<i32> {
        None
    }

    fn getdate(&self) -> Option<String> {
        None
    }
    fn getstring(&self) -> Option<String> {
        None
    }

    fn is_pattern(&self) -> bool {
        false
    }
}

struct F32Entry {
    pub val: f32,
}

impl Entry for F32Entry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) {
        let mystr = pstr
            .clone()
            .into_string()
            .expect(&format!("Error parsing : {:#?} to f32", pstr));
        self.val = mystr
            .parse::<f32>()
            .expect(&format!("Error parsing : {} to f32", mystr));
    }
    fn getf32(&self) -> Option<f32> {
        Some(self.val)
    }
}

struct I32Entry {
    pub val: i32,
}

impl Entry for I32Entry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) {
        let mystr = pstr
            .clone()
            .into_string()
            .expect(&format!("Error parsing : {:#?} to f32", pstr));
        self.val = mystr
            .parse::<i32>()
            .expect(&format!("Error parsing : {} to f32", mystr));
    }
    fn geti32(&self) -> Option<i32> {
        Some(self.val)
    }
}

struct DateEntry {
    pub val: String,
}

impl Entry for DateEntry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) {
        let mystr = pstr
            .clone()
            .into_string()
            .expect(&format!("Error parsing : {:#?} to f32", pstr));

        if chrono::NaiveDate::parse_from_str(&mystr, "%m/%d/%y").is_ok() {
            self.val = mystr;
        }
    }
    fn getdate(&self) -> Option<String> {
        Some(self.val.clone())
    }
}

struct StringEntry {
    pub val: String,
    pub pattern: String,
}

impl Entry for StringEntry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) {
        self.val = pstr
            .clone()
            .into_string()
            .expect(&format!("Error parsing : {:#?} to f32", pstr));
    }
    fn getstring(&self) -> Option<String> {
        Some(self.val.clone())
    }
    fn is_pattern(&self) -> bool {
        self.pattern == self.val
    }
}

fn create_dividend_parsing_sequence(sequence: &mut std::collections::VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "INTC".to_owned(),
    })); // INTC
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Tax Entry
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Income Entry
}

fn create_sold_parsing_sequence(sequence: &mut std::collections::VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(I32Entry { val: 0 })); // Quantity
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Price
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Amount Sold
}

fn create_trade_parsing_sequence(sequence: &mut std::collections::VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(DateEntry { val: String::new() })); // Trade date
    sequence.push_back(Box::new(DateEntry { val: String::new() })); // Settlement date
    sequence.push_back(Box::new(I32Entry { val: 0 })); // MKT /
    sequence.push_back(Box::new(I32Entry { val: 0 })); // / CPT
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "INTC".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "SELL".to_owned(),
    }));
    sequence.push_back(Box::new(I32Entry { val: 0 })); // Quantity
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "$".to_owned(),
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<price>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "Stock".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "Plan".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "PRINCIPAL".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "$".to_owned(),
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<principal>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "INTEL".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "CORP".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "COMMISSION".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "$".to_owned(),
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<commission>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "FEE".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "$".to_owned(),
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<fee>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "NET".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "AMOUNT".to_owned(),
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        pattern: "$".to_owned(),
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<net amount>
}

pub fn parse_brokerage_statement(
    pdftoparse: &str,
) -> (
    Vec<(String, f32, f32)>,
    Vec<(String, String, i32, f32, f32)>,
    Vec<(String, String, i32, f32, f32, f32, f32, f32)>,
) {
    //2. parsing each pdf
    let mypdffile = File::<Vec<u8>>::open(pdftoparse)
        .expect_and_log(&format!("Error opening and parsing file: {}", pdftoparse));

    let mut state = ParserState::SearchingTransactionEntry;
    let mut sequence: std::collections::VecDeque<Box<dyn Entry>> =
        std::collections::VecDeque::new();
    let mut processed_sequence: Vec<Box<dyn Entry>> = vec![];
    // Queue for transaction dates. Pop last one or last two as trade and settlement dates
    let mut transaction_dates: Vec<String> = vec![];
    let mut div_transactions: Vec<(String, f32, f32)> = vec![];
    let mut sold_transactions: Vec<(String, String, i32, f32, f32)> = vec![];
    let mut trades: Vec<(String, String, i32, f32, f32, f32, f32, f32)> = vec![];

    log::info!("Parsing: {} of {} pages", pdftoparse, mypdffile.num_pages());
    for page in mypdffile.pages() {
        let page = page.unwrap();
        let contents = page.contents.as_ref().unwrap();
        for op in contents.operations.iter() {
            match op.operator.as_ref() {
                "TJ" => {
                    // Text show
                    if op.operands.len() > 0 {
                        //transaction_date = op.operands[0];
                        let a = &op.operands[0];
                        match a {
                            Primitive::Array(c) => {
                                for e in c {
                                    if let Primitive::String(actual_string) = e {
                                        match state {
                                            ParserState::SearchingTransactionEntry => {
                                                let rust_string =
                                                    actual_string.clone().into_string().unwrap();
                                                //println!("rust_string: {}", rust_string);
                                                if rust_string == "Dividend" {
                                                    create_dividend_parsing_sequence(&mut sequence);
                                                    state = ParserState::ProcessingTransaction(
                                                        TransactionType::Dividends,
                                                    );
                                                } else if rust_string == "Sold" {
                                                    create_sold_parsing_sequence(&mut sequence);
                                                    state = ParserState::ProcessingTransaction(
                                                        TransactionType::Sold,
                                                    );
                                                } else if rust_string == "TYPE" {
                                                    create_trade_parsing_sequence(&mut sequence);
                                                    state = ParserState::ProcessingTransaction(
                                                        TransactionType::Trade,
                                                    );
                                                } else {
                                                    //if this is date then store it
                                                    if chrono::NaiveDate::parse_from_str(
                                                        &rust_string,
                                                        "%m/%d/%y",
                                                    )
                                                    .is_ok()
                                                    {
                                                        transaction_dates.push(rust_string.clone());
                                                    }
                                                }
                                            }
                                            ParserState::ProcessingTransaction(
                                                transaction_type,
                                            ) => {
                                                // So process transaction element and store it in SOLD
                                                // or DIV
                                                let possible_obj = sequence.pop_front();
                                                match possible_obj {
                                                    // Move executed parser objects into Vector
                                                    // attach only i32 and f32 elements to
                                                    // processed queue
                                                    Some(mut obj) => {
                                                        obj.parse(actual_string);
                                                        // attach to sequence the same string parser if pattern is not met
                                                        if obj.getstring().is_some() {
                                                            if obj.is_pattern() == false {
                                                                sequence.push_front(obj);
                                                            }
                                                        } else {
                                                            processed_sequence.push(obj);
                                                        }
                                                        // If sequence of expected entries is
                                                        // empty then extract data from
                                                        // processeed elements
                                                        if sequence.is_empty() {
                                                            state =
                                                            ParserState::SearchingTransactionEntry;
                                                            let mut transaction =
                                                                processed_sequence.iter();
                                                            match transaction_type {
                                                                TransactionType::Dividends => {
                                                                    let tax_us = transaction.next().unwrap().getf32().expect_and_log("Processing of Dividend transaction went wrong");
                                                                    let gross_us = transaction.next().unwrap().getf32().expect_and_log("Processing of Dividend transaction went wrong");
                                                                    div_transactions.push((
                                                                        transaction_dates.pop().expect("Error: missing transaction dates when parsing"),
                                                                        gross_us,
                                                                        tax_us,
                                                                    ));
                                                                }
                                                                TransactionType::Sold => {
                                                                    let quantity =  transaction.next().unwrap().geti32().expect_and_log("Processing of Sold transaction went wrong");
                                                                    let price = transaction.next().unwrap().getf32().expect_and_log("Processing of Sold transaction went wrong");
                                                                    let amount_sold =  transaction.next().unwrap().getf32().expect_and_log("Prasing of Sold transaction went wrong");
                                                                    // Last transaction date is settlement date
                                                                    // next to last is trade date
                                                                    let settlement_date = transaction_dates.pop().expect("Error: missing trade date when parsing");
                                                                    let trade_date = transaction_dates.pop().expect("Error: missing settlement_date when parsing");

                                                                    sold_transactions.push((
                                                                        trade_date,
                                                                        settlement_date,
                                                                        quantity,
                                                                        price,
                                                                        amount_sold, // net income
                                                                    ));
                                                                }
                                                                TransactionType::Trade => {
                                                                    let transaction_date = transaction.next().unwrap().getdate().expect("Prasing of Trade confirmation went wrong"); // quantity
                                                                    let settlement_date = transaction.next().unwrap().getdate().expect("Prasing of Trade confirmation went wrong"); // quantity
                                                                    transaction.next().unwrap(); // MKT??
                                                                    transaction.next().unwrap(); // CPT??
                                                                    let quantity =  transaction.next().unwrap().geti32().expect("Prasing of Trade confirmation went wrong"); // quantity
                                                                    let price = transaction.next().unwrap().getf32().expect("Prasing of Trade confirmation went wrong"); // price
                                                                    let principal = transaction.next().unwrap().getf32().expect("Prasing of Trade confirmation went wrong"); // principal
                                                                    let commission = transaction.next().unwrap().getf32().expect("Prasing of Trade confirmation went wrong"); // commission
                                                                    let fee = transaction.next().unwrap().getf32().expect("Prasing of Trade confirmation went wrong"); // fee
                                                                    let net = transaction.next().unwrap().getf32().expect("Prasing of Trade confirmation went wrong"); // net
                                                                    trades.push((
                                                                        transaction_date,
                                                                        settlement_date,
                                                                        quantity,
                                                                        price,
                                                                        principal,
                                                                        commission,
                                                                        fee,
                                                                        net,
                                                                    ));
                                                                }
                                                            }
                                                            processed_sequence.clear();
                                                        } else {
                                                            state =
                                                                ParserState::ProcessingTransaction(
                                                                    transaction_type,
                                                                );
                                                        }
                                                    }

                                                    // In nothing more to be done then just extract
                                                    // parsed data from paser objects
                                                    None => {
                                                        state = ParserState::ProcessingTransaction(
                                                            transaction_type,
                                                        );
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            _ => (),
                        }
                    }
                }
                _ => {}
            }
        }
    }
    (div_transactions, sold_transactions, trades)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore]
    fn test_parse_brokerage_statement() -> Result<(), String> {
        assert_eq!(
            parse_brokerage_statement("data/example-divs.pdf"),
            (
                vec![("03/01/22".to_owned(), 698.25, 104.74)],
                vec![],
                vec![]
            )
        );
        assert_eq!(
            parse_brokerage_statement("data/example-sold-wire.pdf"),
            (
                vec![],
                vec![(
                    "05/02/22".to_owned(),
                    "05/04/22".to_owned(),
                    -1,
                    43.69,
                    43.67
                )],
                vec![]
            )
        );

        //TODO(jczaja): Renable reinvest dividends case as soon as you get some PDFs
        //assert_eq!(
        //    parse_brokerage_statement("data/example3.pdf"),
        //    (
        //        vec![
        //            ("06/01/21".to_owned(), 0.17, 0.03),
        //            ("06/01/21".to_owned(), 45.87, 6.88)
        //        ],
        //        vec![],
        //        vec![]
        //    )
        //);

        //assert_eq!(
        //    parse_brokerage_statement("data/example5.pdf"),
        //    (
        //        vec![],
        //        vec![],
        //        vec![(
        //            "04/11/22".to_owned(),
        //            "04/13/22".to_owned(),
        //            1,
        //            46.92,
        //            46.92,
        //            0.01,
        //           0.01,
        //            46.9
        //        )]
        //    )
        //);
        Ok(())
    }
}
