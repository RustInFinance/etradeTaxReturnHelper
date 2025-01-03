use pdf::file::File;
use pdf::object::PageRc;
use pdf::primitive::Primitive;

pub use crate::logging::ResultExt;

#[derive(Clone, Debug, PartialEq)]
enum StatementType {
    UnknownDocument,
    BrokerageStatement,
    AccountStatement,
}

#[derive(Clone, Debug, PartialEq)]
enum TransactionType {
    Interests,
    Dividends,
    Sold,
    Tax,
    Trade,
}

#[derive(Debug, PartialEq)]
enum ParserState {
    SearchingCashFlowBlock,
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
        // Extracted string should have "," removed and then be parsed
        self.val = mystr
            .trim()
            .replace(",", "")
            .replace("(", "")
            .replace(")", "")
            .replace("$", "")
            .parse::<f32>()
            .expect(&format!("Error parsing : {} to f32", mystr));
        log::info!("Parsed f32 value: {}", self.val);
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
            .expect(&format!("Error parsing : {:#?} to i32", pstr));
        self.val = mystr
            .parse::<i32>()
            .expect(&format!("Error parsing : {} to i32", mystr));
        log::info!("Parsed i32 value: {}", self.val);
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
            .expect(&format!("Error parsing : {:#?} to Data", pstr));

        if chrono::NaiveDate::parse_from_str(&mystr, "%m/%d/%y").is_ok() {
            self.val = mystr;
            log::info!("Parsed date value: {}", self.val);
        }
    }
    fn getdate(&self) -> Option<String> {
        Some(self.val.clone())
    }
}

struct StringEntry {
    pub val: String,
    pub patterns: Vec<String>,
}

impl Entry for StringEntry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) {
        self.val = pstr
            .clone()
            .into_string()
            .expect(&format!("Error parsing : {:#?} to String", pstr));
        log::info!("Parsed String value: {}", self.val);
    }
    fn getstring(&self) -> Option<String> {
        Some(self.val.clone())
    }
    // Either match parsed token against any of patterns or in case no patterns are there
    // just return match (true)
    fn is_pattern(&self) -> bool {
        self.patterns.len() == 0 || self.patterns.iter().find(|&x| self.val == *x).is_some()
    }
}

fn create_dividend_parsing_sequence(sequence: &mut std::collections::VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["INTC".to_owned(), "DLB".to_owned()],
    })); // INTC, DLB
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Tax Entry
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Income Entry
}

fn create_tax_parsing_sequence(sequence: &mut std::collections::VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["INTEL CORP".to_owned(), "ADVANCED MICRO DEVICES".to_owned()],
    }));
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Tax Entry
}

fn create_interests_fund_parsing_sequence(
    sequence: &mut std::collections::VecDeque<Box<dyn Entry>>,
) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["TREASURY LIQUIDITY FUND".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![
            "DIV PAYMENT".to_owned(),
            "Transaction Reportable for the Prior Year.".to_owned(),
        ],
    }));
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Income Entry
}

fn create_interest_adjustment_parsing_sequence(
    sequence: &mut std::collections::VecDeque<Box<dyn Entry>>,
) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![],
    }));
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Income Entry
}

fn create_qualified_dividend_parsing_sequence(
    sequence: &mut std::collections::VecDeque<Box<dyn Entry>>,
) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["INTEL CORP".to_owned()],
    }));
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Income Entry
}

fn create_sold_parsing_sequence(sequence: &mut std::collections::VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Quantity
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Price
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Amount Sold
}

fn create_sold_2_parsing_sequence(sequence: &mut std::collections::VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["INTEL CORP".to_owned(), "ADVANCED MICRO DEVICES".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["ACTED AS AGENT".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["UNSOLICITED TRADE".to_owned()],
    }));
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // Quantity
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
        patterns: vec!["INTC".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["SELL".to_owned()],
    }));
    sequence.push_back(Box::new(I32Entry { val: 0 })); // Quantity
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["$".to_owned()],
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<price>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["Stock".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["Plan".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["PRINCIPAL".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["$".to_owned()],
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<principal>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["INTEL".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["CORP".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["COMMISSION".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["$".to_owned()],
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<commission>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["FEE".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["$".to_owned()],
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<fee>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["NET".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["AMOUNT".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["$".to_owned()],
    })); // $...
    sequence.push_back(Box::new(F32Entry { val: 0.0 })); // ..<net amount>
}

fn yield_sold_transaction(
    transaction: &mut std::slice::Iter<'_, Box<dyn Entry>>,
    transaction_dates: &mut Vec<String>,
) -> Option<(String, String, f32, f32, f32)> {
    let quantity = transaction
        .next()
        .unwrap()
        .getf32()
        .expect_and_log("Processing of Sold transaction went wrong");
    let price = transaction
        .next()
        .unwrap()
        .getf32()
        .expect_and_log("Processing of Sold transaction went wrong");
    let amount_sold = transaction
        .next()
        .unwrap()
        .getf32()
        .expect_and_log("Parsing of Sold transaction went wrong");
    // Last transaction date is settlement date
    // next to last is trade date
    let (trade_date, settlement_date) = match transaction_dates.len() {
        1 => {
            log::info!("Detected unsettled sold transaction. Skipping");
            return None;
        }
        0 => {
            log::error!(
                "Error parsing transaction & settlement dates. Number of parsed dates: {}",
                transaction_dates.len()
            );
            panic!("Error processing sold transaction. Exitting!")
        }
        _ => {
            let settlement_date = transaction_dates
                .pop()
                .expect("Error: missing trade date when parsing");
            let trade_date = transaction_dates
                .pop()
                .expect("Error: missing settlement_date when parsing");
            (trade_date, settlement_date)
        }
    };

    Some((trade_date, settlement_date, quantity, price, amount_sold))
}

/// Recognize whether PDF document is of Brokerage Statement type (old e-trade type of PDF
/// document) or maybe Single account statment (newer e-trade/morgan stanley type of document)
fn recognize_statement(page: PageRc) -> Result<StatementType, String> {
    log::info!("Starting to recognize PDF document type");
    let contents = page
        .contents
        .as_ref()
        .ok_or("Unable to get content of first PDF page")?;

    let mut statement_type = StatementType::UnknownDocument;
    contents.operations.iter().try_for_each(|op| {
        log::trace!("Detected PDF command: {}",op.operator);
        match op.operator.as_ref() {
            "TJ" => {
                // Text show
                if op.operands.len() > 0 {
                    //transaction_date = op.operands[0];
                    let a = &op.operands[0];
                    log::trace!("Detected PDF text object: {a}");
                    match a {
                        Primitive::Array(c) => {
                            for e in c {
                                if let Primitive::String(actual_string) = e {
                                    let raw_string = actual_string.clone().into_string();
                                    let rust_string = if let Ok(r) = raw_string {
                                        r.trim().to_uppercase()
                                    } else {
                                        "".to_owned()
                                    };
                                    if rust_string.contains("ACCT:")  {
                                        statement_type = StatementType::BrokerageStatement;
                                        log::info!("PDF parser recognized Brokerage Statement document by finding: \"{rust_string}\"");
                                        return Ok(());
                                    }
                                }
                            }
                        }
                        _ => (),
                    }
                }
            },
            "Tj" => {
                // Text show
                if op.operands.len() > 0 {
                    //transaction_date = op.operands[0];
                    let a = &op.operands[0];
                    log::info!("Detected PDF text object: {a}");
                    match a {
                        Primitive::String(actual_string) => {
                            let raw_string = actual_string.clone().into_string();
                            let rust_string = if let Ok(r) = raw_string {
                                r.trim().to_uppercase()
                            } else {
                                "".to_owned()
                            };

                            if rust_string == "CLIENT STATEMENT" {
                                statement_type = StatementType::AccountStatement;
                                log::info!("PDF parser recognized Account Statement document by finding: \"{rust_string}\"");
                                return Ok(());
                            }
                        },

                        _ => (),
                    }
                }
            }
            _ => {}
        }
        Ok::<(),String>(())
    })?;

    Ok(statement_type)
}

fn process_transaction(
    interests_transactions: &mut Vec<(String, f32)>,
    div_transactions: &mut Vec<(String, f32, f32)>,
    sold_transactions: &mut Vec<(String, String, f32, f32, f32)>,
    actual_string: &pdf::primitive::PdfString,
    transaction_dates: &mut Vec<String>,
    processed_sequence: &mut Vec<Box<dyn Entry>>,
    sequence: &mut std::collections::VecDeque<Box<dyn Entry>>,
    transaction_type: TransactionType,
) -> Result<ParserState, String> {
    let state;
    let possible_obj = sequence.pop_front();
    match possible_obj {
        // Move executed parser objects into Vector
        // attach only i32 and f32 elements to
        // processed queue
        Some(mut obj) => {
            obj.parse(actual_string);
            // attach to sequence the same string parser if pattern is not met
            match obj.getstring() {
                Some(token) => {
                    if obj.is_pattern() == false && token != "$" {
                        sequence.push_front(obj);
                    }
                }

                None => processed_sequence.push(obj),
            }

            // If sequence of expected entries is
            // empty then extract data from
            // processeed elements
            if sequence.is_empty() {
                state = ParserState::SearchingTransactionEntry;
                let mut transaction = processed_sequence.iter();
                match transaction_type {
                    TransactionType::Tax => {
                        // Ok we assume here that taxation of transaction appears later in document
                        // than actual transaction that is a subject to taxation
                        let tax_us = transaction
                            .next()
                            .unwrap()
                            .getf32()
                            .ok_or("Processing of Tax transaction went wrong")?;

                        // Here we just go through registered transactions and pick the one where
                        // income is higher than tax and apply tax value and where tax was not yet
                        // applied
                        let subject_to_tax = div_transactions
                            .iter_mut()
                            .find(|x| x.1 > tax_us && x.2 == 0.0f32)
                            .ok_or("Error: Unable to find transaction that was taxed")?;
                        log::info!("Tax: {tax_us} was applied to {subject_to_tax:?}");
                        subject_to_tax.2 = tax_us;
                        log::info!("Completed parsing Tax transaction");
                    }
                    TransactionType::Interests => {
                        let gross_us = transaction
                            .next()
                            .unwrap()
                            .getf32()
                            .ok_or("Processing of Interests transaction went wrong")?;

                        interests_transactions.push((
                            transaction_dates
                                .pop()
                                .ok_or("Error: missing transaction dates when parsing")?,
                            gross_us,
                        ));
                        log::info!("Completed parsing Interests transaction");
                    }
                    TransactionType::Dividends => {
                        let gross_us = transaction
                            .next()
                            .unwrap()
                            .getf32()
                            .ok_or("Processing of Dividend transaction went wrong")?;

                        div_transactions.push((
                            transaction_dates
                                .pop()
                                .ok_or("Error: missing transaction dates when parsing")?,
                            gross_us,
                            0.0, // No tax info yet. It will be added later in Tax section
                        ));
                        log::info!("Completed parsing Dividend transaction");
                    }
                    TransactionType::Sold => {
                        if let Some(trans_details) =
                            yield_sold_transaction(&mut transaction, transaction_dates)
                        {
                            sold_transactions.push(trans_details);
                        }
                        log::info!("Completed parsing Sold transaction");
                    }
                    TransactionType::Trade => {
                        return Err("TransactionType::Trade should not appear during account statement processing!".to_string());
                    }
                }
                processed_sequence.clear();
            } else {
                state = ParserState::ProcessingTransaction(transaction_type);
            }
        }

        // In nothing more to be done then just extract
        // parsed data from paser objects
        None => {
            state = ParserState::ProcessingTransaction(transaction_type);
        }
    }
    Ok(state)
}

/// Parse borkerage statement document type
fn parse_brokerage_statement<'a, I>(
    pages_iter: I,
) -> Result<
    (
        Vec<(String, f32)>,
        Vec<(String, f32, f32)>,
        Vec<(String, String, f32, f32, f32)>,
        Vec<(String, String, i32, f32, f32, f32, f32, f32)>,
    ),
    String,
>
where
    I: Iterator<Item = Result<PageRc, pdf::error::PdfError>>,
{
    let mut div_transactions: Vec<(String, f32, f32)> = vec![];
    let mut sold_transactions: Vec<(String, String, f32, f32, f32)> = vec![];
    let mut trades: Vec<(String, String, i32, f32, f32, f32, f32, f32)> = vec![];
    let mut state = ParserState::SearchingTransactionEntry;
    let mut sequence: std::collections::VecDeque<Box<dyn Entry>> =
        std::collections::VecDeque::new();
    let mut processed_sequence: Vec<Box<dyn Entry>> = vec![];
    // Queue for transaction dates. Pop last one or last two as trade and settlement dates
    let mut transaction_dates: Vec<String> = vec![];

    for page in pages_iter {
        let page = page.unwrap();
        let contents = page.contents.as_ref().unwrap();
        for op in contents.operations.iter() {
            match op.operator.as_ref() {
                "TJ" => {
                    // Text show
                    if op.operands.len() > 0 {
                        //transaction_date = op.operands[0];
                        let a = &op.operands[0];
                        log::trace!("Detected PDF object: {a}");
                        match a {
                            Primitive::Array(c) => {
                                for e in c {
                                    if let Primitive::String(actual_string) = e {
                                        match state {
                                            ParserState::SearchingCashFlowBlock => {
                                                log::error!("Brokerage documents do not have cashflow  block!")
                                            }
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
                                                                TransactionType::Tax => {
                                                                    return Err("TransactionType::Tax should not appear during brokerage statement processing!".to_string());
                                                                }
                                                                TransactionType::Interests => {
                                                                    return Err("TransactionType::Interest rate should not appear during brokerage statement processing!".to_string());
                                                                }
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
                                                                    if let Some(trans_details) =
                                                                        yield_sold_transaction(
                                                                            &mut transaction,
                                                                            &mut transaction_dates,
                                                                        )
                                                                    {
                                                                        sold_transactions
                                                                            .push(trans_details);
                                                                    }
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
    Ok((vec![], div_transactions, sold_transactions, trades))
}

fn check_if_transaction(
    candidate_string: &str,
    dates: &mut Vec<String>,
    sequence: &mut std::collections::VecDeque<Box<dyn Entry>>,
    year: Option<String>,
) -> Result<ParserState, String> {
    let mut state = ParserState::SearchingTransactionEntry;

    log::info!("Searching for transaction through: \"{candidate_string}\"");

    let actual_year =
        year.ok_or("Missing year that should be parsed before transactions".to_owned())?;

    if candidate_string == "DIVIDEND" {
        create_interests_fund_parsing_sequence(sequence);
        state = ParserState::ProcessingTransaction(TransactionType::Interests);
        log::info!("Starting to parse Interests transaction");
    } else if candidate_string == "INTEREST INCOME-ADJ" {
        create_interest_adjustment_parsing_sequence(sequence);
        state = ParserState::ProcessingTransaction(TransactionType::Interests);
        log::info!("Starting to parse Interest adjustment transaction");
    } else if candidate_string == "QUALIFIED DIVIDEND" {
        create_qualified_dividend_parsing_sequence(sequence);
        state = ParserState::ProcessingTransaction(TransactionType::Dividends);
        log::info!("Starting to parse Qualified Dividend transaction");
    } else if candidate_string == "SOLD" {
        create_sold_2_parsing_sequence(sequence);
        state = ParserState::ProcessingTransaction(TransactionType::Sold);
        log::info!("Starting to parse Sold transaction");
    } else if candidate_string == "TAX WITHHOLDING" {
        create_tax_parsing_sequence(sequence);
        state = ParserState::ProcessingTransaction(TransactionType::Tax);
        log::info!("Starting to parse Tax transaction");
    } else if candidate_string == "NET CREDITS/(DEBITS)" {
        // "NET CREDITS/(DEBITS)" is marking the end of CASH FLOW ACTIVITIES block
        state = ParserState::SearchingCashFlowBlock;
        log::info!("Finished parsing transactions");
    } else {
        let datemonth_pattern =
            regex::Regex::new(r"^(0?[1-9]|1[012])/(0?[1-9]|[12][0-9]|3[01])$").unwrap();
        if datemonth_pattern.is_match(candidate_string) {
            dates.push(candidate_string.to_owned() + "/" + actual_year.as_str());
        }
    }
    Ok(state)
}

/// Get las two digits of year from pattern like:  "(AS OF 12/31/23)"
fn yield_year(rust_string: &str) -> Option<String> {
    let period_pattern = regex::Regex::new(r"\d{2}\)").unwrap();
    match period_pattern.find(rust_string) {
        Some(x) => {
            let year_str = x.as_str();
            let last_two_digits = &year_str[..year_str.len() - 1];
            Some(last_two_digits.to_string())
        }
        None => None,
    }
}

/// Parse borkerage statement document type
fn parse_account_statement<'a, I>(
    pages_iter: I,
) -> Result<
    (
        Vec<(String, f32)>,
        Vec<(String, f32, f32)>,
        Vec<(String, String, f32, f32, f32)>,
        Vec<(String, String, i32, f32, f32, f32, f32, f32)>,
    ),
    String,
>
where
    I: Iterator<Item = Result<PageRc, pdf::error::PdfError>>,
{
    let mut interests_transactions: Vec<(String, f32)> = vec![];
    let mut div_transactions: Vec<(String, f32, f32)> = vec![];
    let mut sold_transactions: Vec<(String, String, f32, f32, f32)> = vec![];
    let trades: Vec<(String, String, i32, f32, f32, f32, f32, f32)> = vec![];
    let mut state = ParserState::SearchingCashFlowBlock;
    let mut sequence: std::collections::VecDeque<Box<dyn Entry>> =
        std::collections::VecDeque::new();
    let mut processed_sequence: Vec<Box<dyn Entry>> = vec![];
    // Queue for transaction dates. Pop last one or last two as trade and settlement dates
    let mut transaction_dates: Vec<String> = vec![];
    let mut year: Option<String> = None;

    for page in pages_iter {
        let page = page.unwrap();
        let contents = page.contents.as_ref().unwrap();
        for op in contents.operations.iter() {
            match op.operator.as_ref() {
                "Tj" => {
                    // Text show
                    if op.operands.len() > 0 {
                        //transaction_date = op.operands[0];
                        let a = &op.operands[0];
                        log::trace!("Parsing account statement: Detected PDF object: {a}");
                        match a {
                            Primitive::String(actual_string) => {
                                let raw_string = actual_string.clone().into_string();
                                let rust_string = if let Ok(r) = raw_string {
                                    r.trim().to_uppercase().replace("$", "")
                                } else {
                                    "".to_owned()
                                };
                                // Ignore empty tokens
                                if rust_string != "" {
                                    match state {
                                        ParserState::SearchingCashFlowBlock => {
                                            // Pattern to match "(AS OF <date in a format like 12/31/23>)"
                                            let date_pattern = regex::Regex::new(r"\(AS OF (\d{1,2}\/\d{1,2}\/\d{2})\)").map_err(|_| "Unable to create regular expression to capture fiscal year")?;

                                            // When we find "CASH FLOW ACTIVITY BY DATE" then
                                            // it is a starting point of transactions we are
                                            // interested in
                                            if rust_string == "CASH FLOW ACTIVITY BY DATE" {
                                                state = ParserState::SearchingTransactionEntry;
                                                log::info!("Parsing account statement: \"CASH FLOW ACTIVITY BY DATE\" detected. Start to parse transactions");
                                            } else if date_pattern.is_match(rust_string.as_str())
                                                && year.is_none()
                                            {
                                                // If we find (AS OF <date e.g. 12/01/2023>))
                                                // get year (last two digits out of it)
                                                year = yield_year(&rust_string);
                                            }
                                        }
                                        ParserState::SearchingTransactionEntry => {
                                            state = check_if_transaction(
                                                &rust_string,
                                                &mut transaction_dates,
                                                &mut sequence,
                                                year.clone(),
                                            )?;
                                        }
                                        ParserState::ProcessingTransaction(transaction_type) => {
                                            state = process_transaction(
                                                &mut interests_transactions,
                                                &mut div_transactions,
                                                &mut sold_transactions,
                                                &actual_string,
                                                &mut transaction_dates,
                                                &mut processed_sequence,
                                                &mut sequence,
                                                transaction_type,
                                            )?
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

    Ok((
        interests_transactions,
        div_transactions,
        sold_transactions,
        trades,
    ))
}
///  This function parses given PDF document
///  and returns result of parsing which is a tuple of
///  interest rate transactions
///  found Dividends paid transactions (div_transactions),
///  Sold stock transactions (sold_transactions)
///  information on transactions in case of parsing trade document (trades)
///  Dividends paid transaction is:
///        transaction date, gross_us, tax_us,
///  Sold stock transaction is :
///     (trade_date, settlement_date, quantity, price, amount_sold)
pub fn parse_statement(
    pdftoparse: &str,
) -> Result<
    (
        Vec<(String, f32)>,
        Vec<(String, f32, f32)>,
        Vec<(String, String, f32, f32, f32)>,
        Vec<(String, String, i32, f32, f32, f32, f32, f32)>,
    ),
    String,
> {
    //2. parsing each pdf
    let mypdffile = File::<Vec<u8>>::open(pdftoparse)
        .map_err(|_| format!("Error opening and parsing file: {}", pdftoparse))?;

    log::info!("Parsing: {} of {} pages", pdftoparse, mypdffile.num_pages());

    let mut pdffile_iter = mypdffile.pages();

    let first_page = pdffile_iter
        .next()
        .unwrap()
        .map_err(|_| "Unable to get first page of PDF file".to_string())?;

    let document_type = recognize_statement(first_page)?;

    let (interests_transactions, div_transactions, sold_transactions, trades) = match document_type
    {
        StatementType::UnknownDocument => {
            log::info!("Processing unknown document PDF");
            return Err(format!("Unsupported PDF document type: {pdftoparse}"));
        }
        StatementType::BrokerageStatement => {
            log::info!("Processing brokerage statement PDF");
            parse_brokerage_statement(pdffile_iter)?
        }
        StatementType::AccountStatement => {
            log::info!("Processing Account statement PDF");
            parse_account_statement(pdffile_iter)?
        }
    };

    Ok((
        interests_transactions,
        div_transactions,
        sold_transactions,
        trades,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser() -> Result<(), String> {
        // quantity
        let data: Vec<u8> = vec!['1' as u8];
        let mut i = I32Entry { val: 0 };
        i.parse(&pdf::primitive::PdfString::new(data));
        assert_eq!(i.geti32(), Some(1));

        // price
        let data: Vec<u8> = vec![
            '2' as u8, '8' as u8, '.' as u8, '2' as u8, '0' as u8, '3' as u8, '5' as u8,
        ];
        let mut f = F32Entry { val: 0.0 };
        f.parse(&pdf::primitive::PdfString::new(data));
        assert_eq!(f.getf32(), Some(28.2035));

        // amount
        let data: Vec<u8> = vec![
            '4' as u8, ',' as u8, '8' as u8, '7' as u8, '7' as u8, '.' as u8, '3' as u8, '6' as u8,
        ];
        let mut f = F32Entry { val: 0.0 };
        f.parse(&pdf::primitive::PdfString::new(data));
        assert_eq!(f.getf32(), Some(4877.36));

        let data: Vec<u8> = vec![
            '(' as u8, '5' as u8, '7' as u8, '.' as u8, '9' as u8, '8' as u8, ')' as u8,
        ];
        let mut f = F32Entry { val: 0.0 };
        f.parse(&pdf::primitive::PdfString::new(data));
        assert_eq!(f.getf32(), Some(57.98));

        let data: Vec<u8> = vec!['$' as u8, '1' as u8, '.' as u8, '2' as u8, '2' as u8];
        let mut f = F32Entry { val: 0.0 };
        f.parse(&pdf::primitive::PdfString::new(data));
        assert_eq!(f.getf32(), Some(1.22));

        let data: Vec<u8> = vec![
            '8' as u8, '2' as u8, '.' as u8, '0' as u8, '0' as u8, '0' as u8,
        ];
        let mut f = F32Entry { val: 0.0 };
        f.parse(&pdf::primitive::PdfString::new(data));
        assert_eq!(f.getf32(), Some(82.00));

        // company code
        let data: Vec<u8> = vec!['D' as u8, 'L' as u8, 'B' as u8];
        let mut s = StringEntry {
            val: String::new(),
            patterns: vec!["INTC".to_owned(), "DLB".to_owned()],
        };
        s.parse(&pdf::primitive::PdfString::new(data));
        assert_eq!(s.is_pattern(), true);

        // unimportant string
        let data: Vec<u8> = vec!['K' as u8, 'L' as u8, 'M' as u8];
        let mut s = StringEntry {
            val: String::new(),
            patterns: vec![],
        };
        s.parse(&pdf::primitive::PdfString::new(data));
        assert_eq!(s.is_pattern(), true);
        Ok(())
    }

    #[test]
    fn test_transaction_validation() -> Result<(), String> {
        let mut transaction_dates: Vec<String> =
            vec!["11/29/22".to_string(), "12/01/22".to_string()];
        let mut sequence: std::collections::VecDeque<Box<dyn Entry>> =
            std::collections::VecDeque::new();
        create_sold_parsing_sequence(&mut sequence);
        let mut processed_sequence: Vec<Box<dyn Entry>> = vec![];
        processed_sequence.push(Box::new(F32Entry { val: 42.0 })); //quantity
        processed_sequence.push(Box::new(F32Entry { val: 28.8400 })); // Price
        processed_sequence.push(Box::new(F32Entry { val: 1210.83 })); // Amount Sold

        yield_sold_transaction(&mut processed_sequence.iter(), &mut transaction_dates)
            .ok_or("Parsing error".to_string())?;
        Ok(())
    }

    #[test]
    fn test_transaction_validation_more_dates() -> Result<(), String> {
        let mut transaction_dates: Vec<String> = vec![
            "11/28/22".to_string(),
            "11/29/22".to_string(),
            "12/01/22".to_string(),
        ];
        let mut sequence: std::collections::VecDeque<Box<dyn Entry>> =
            std::collections::VecDeque::new();
        create_sold_parsing_sequence(&mut sequence);
        let mut processed_sequence: Vec<Box<dyn Entry>> = vec![];
        processed_sequence.push(Box::new(F32Entry { val: 42.0 })); //quantity
        processed_sequence.push(Box::new(F32Entry { val: 28.8400 })); // Price
        processed_sequence.push(Box::new(F32Entry { val: 1210.83 })); // Amount Sold

        yield_sold_transaction(&mut processed_sequence.iter(), &mut transaction_dates)
            .ok_or("Parsing error".to_string())?;
        Ok(())
    }

    #[test]
    fn test_unsettled_transaction_validation() -> Result<(), String> {
        let mut transaction_dates: Vec<String> = vec!["11/29/22".to_string()];
        let mut sequence: std::collections::VecDeque<Box<dyn Entry>> =
            std::collections::VecDeque::new();
        create_sold_parsing_sequence(&mut sequence);
        let mut processed_sequence: Vec<Box<dyn Entry>> = vec![];
        processed_sequence.push(Box::new(F32Entry { val: 42.0 })); //quantity
        processed_sequence.push(Box::new(F32Entry { val: 28.8400 })); // Price
        processed_sequence.push(Box::new(F32Entry { val: 1210.83 })); // Amount Sold

        assert_eq!(
            yield_sold_transaction(&mut processed_sequence.iter(), &mut transaction_dates),
            None
        );
        Ok(())
    }

    #[test]
    fn test_check_if_transaction() -> Result<(), String> {
        let rust_string = "DIVIDEND";
        let mut transaction_dates = vec![];
        let mut sequence = std::collections::VecDeque::new();

        assert_eq!(
            check_if_transaction(
                &rust_string,
                &mut transaction_dates,
                &mut sequence,
                Some("23".to_owned())
            ),
            Ok(ParserState::ProcessingTransaction(
                TransactionType::Interests
            ))
        );

        let rust_string = "QUALIFIED DIVIDEND";
        assert_eq!(
            check_if_transaction(
                &rust_string,
                &mut transaction_dates,
                &mut sequence,
                Some("23".to_owned())
            ),
            Ok(ParserState::ProcessingTransaction(
                TransactionType::Dividends
            ))
        );

        let rust_string = "QUALIFIED DIVIDEND";
        assert_eq!(
            check_if_transaction(&rust_string, &mut transaction_dates, &mut sequence, None),
            Err("Missing year that should be parsed before transactions".to_owned())
        );

        let rust_string = "CASH";
        assert_eq!(
            check_if_transaction(
                &rust_string,
                &mut transaction_dates,
                &mut sequence,
                Some("23".to_owned())
            ),
            Ok(ParserState::SearchingTransactionEntry)
        );

        Ok(())
    }

    #[test]
    fn test_yield_year() -> Result<(), String> {
        let rust_string = "(AS OF 12/31/23)";
        assert_eq!(yield_year(&rust_string), Some("23".to_owned()));
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_recognize_document_type_ms() -> Result<(), String> {
        let pdftoparse = "etrade_data_2023/MS_ClientStatements_6557_202309.pdf";

        //2. parsing each pdf
        let mypdffile = File::<Vec<u8>>::open(pdftoparse)
            .map_err(|_| format!("Error opening and parsing file: {}", pdftoparse))?;

        let mut pdffile_iter = mypdffile.pages();

        let first_page = pdffile_iter
            .next()
            .unwrap()
            .map_err(|_| "Unable to get first page of PDF file".to_string())?;

        let document_type = recognize_statement(first_page)?;

        assert_eq!(document_type, StatementType::AccountStatement);

        Ok(())
    }

    #[test]
    #[ignore]
    fn test_recognize_document_type_bs() -> Result<(), String> {
        let pdftoparse = "etrade_data_2023/Brokerage Statement - XXXXX6557 - 202302.pdf";

        //2. parsing each pdf
        let mypdffile = File::<Vec<u8>>::open(pdftoparse)
            .map_err(|_| format!("Error opening and parsing file: {}", pdftoparse))?;

        let mut pdffile_iter = mypdffile.pages();

        let first_page = pdffile_iter
            .next()
            .unwrap()
            .map_err(|_| "Unable to get first page of PDF file".to_string())?;

        let document_type = recognize_statement(first_page)?;

        assert_eq!(document_type, StatementType::BrokerageStatement);

        Ok(())
    }

    #[test]
    fn test_recognize_document_type_unk() -> Result<(), String> {
        let pdftoparse = "data/HowToReadETfromMSStatement.pdf";

        //2. parsing each pdf
        let mypdffile = File::<Vec<u8>>::open(pdftoparse)
            .map_err(|_| format!("Error opening and parsing file: {}", pdftoparse))?;

        let mut pdffile_iter = mypdffile.pages();

        let first_page = pdffile_iter
            .next()
            .unwrap()
            .map_err(|_| "Unable to get first page of PDF file".to_string())?;

        let document_type = recognize_statement(first_page)?;

        assert_eq!(document_type, StatementType::UnknownDocument);

        Ok(())
    }

    #[test]
    #[ignore]
    fn test_account_statement() -> Result<(), String> {
        assert_eq!(
            parse_statement("data/MS_ClientStatements_6557_202312.pdf"),
            (Ok((
                vec![("12/1/23".to_owned(), 1.22)],
                vec![("12/1/23".to_owned(), 386.50, 57.98),],
                vec![(
                    "12/21/23".to_owned(),
                    "12/26/23".to_owned(),
                    82.0,
                    46.45,
                    3808.86
                )],
                vec![]
            )))
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_combined_account_statement() -> Result<(), String> {
        assert_eq!(
            parse_statement("etrade_data_2024/ClientStatements_010325.pdf"),
            (Ok((
                vec![
                    ("12/2/24".to_owned(), 4.88),
                    ("10/1/24".to_owned(), 24.91),
                    ("11/1/24".to_owned(), 25.09),
                    ("9/3/24".to_owned(), 23.65), // Interest rates
                    ("8/1/24".to_owned(), 4.34),
                    ("7/1/24".to_owned(), 3.72),
                    ("6/3/24".to_owned(), 13.31),
                    ("5/1/24".to_owned(), 0.62),
                    ("4/1/24".to_owned(), 1.16),
                    ("1/2/24".to_owned(), 0.49)
                ],
                vec![
                    ("6/3/24".to_owned(), 57.25, 8.59), // Dividends date, gross, tax_us
                    ("3/1/24".to_owned(), 380.25, 57.04)
                ],
                vec![
                    (
                        "12/4/24".to_owned(),
                        "12/5/24".to_owned(),
                        30.0,
                        22.5,
                        674.98
                    ),
                    (
                        "12/5/24".to_owned(),
                        "12/6/24".to_owned(),
                        55.0,
                        21.96,
                        1207.76
                    ),
                    (
                        "11/1/24".to_owned(),
                        "11/4/24".to_owned(),
                        15.0,
                        23.32,
                        349.79
                    ),
                    (
                        "9/3/24".to_owned(),
                        "9/4/24".to_owned(),
                        17.0,
                        21.53,
                        365.99
                    ), // Sold
                    (
                        "9/9/24".to_owned(),
                        "9/10/24".to_owned(),
                        14.0,
                        18.98,
                        265.71
                    ),
                    (
                        "8/5/24".to_owned(),
                        "8/6/24".to_owned(),
                        14.0,
                        20.21,
                        282.93
                    ),
                    (
                        "8/20/24".to_owned(),
                        "8/21/24".to_owned(),
                        328.0,
                        21.0247,
                        6895.89
                    ),
                    (
                        "7/31/24".to_owned(),
                        "8/1/24".to_owned(),
                        151.0,
                        30.44,
                        4596.31
                    ),
                    (
                        "6/3/24".to_owned(),
                        "6/4/24".to_owned(),
                        14.0,
                        31.04,
                        434.54
                    ),
                    (
                        "5/1/24".to_owned(),
                        "5/3/24".to_owned(),
                        126.0,
                        30.14,
                        3797.6
                    ),
                    (
                        "5/1/24".to_owned(),
                        "5/3/24".to_owned(),
                        124.0,
                        30.14,
                        3737.33
                    ),
                    (
                        "5/1/24".to_owned(),
                        "5/3/24".to_owned(),
                        89.0,
                        30.6116,
                        2724.4
                    ),
                    (
                        "5/2/24".to_owned(),
                        "5/6/24".to_owned(),
                        182.0,
                        30.56,
                        5561.87
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        440.0,
                        30.835,
                        13567.29
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        198.0,
                        30.835,
                        6105.28
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        146.0,
                        30.8603,
                        4505.56
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        145.0,
                        30.8626,
                        4475.04
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        75.0,
                        30.815,
                        2311.11
                    ),
                    (
                        "5/6/24".to_owned(),
                        "5/8/24".to_owned(),
                        458.0,
                        31.11,
                        14248.26
                    ),
                    (
                        "5/31/24".to_owned(),
                        "6/3/24".to_owned(),
                        18.0,
                        30.22,
                        543.94
                    ),
                    (
                        "4/3/24".to_owned(),
                        "4/5/24".to_owned(),
                        31.0,
                        40.625,
                        1259.36
                    ),
                    (
                        "4/11/24".to_owned(),
                        "4/15/24".to_owned(),
                        209.0,
                        37.44,
                        7824.89
                    ),
                    (
                        "4/11/24".to_owned(),
                        "4/15/24".to_owned(),
                        190.0,
                        37.44,
                        7113.54
                    ),
                    (
                        "4/16/24".to_owned(),
                        "4/18/24".to_owned(),
                        310.0,
                        36.27,
                        11243.61
                    ),
                    (
                        "4/29/24".to_owned(),
                        "5/1/24".to_owned(),
                        153.0,
                        31.87,
                        4876.07
                    ),
                    (
                        "4/29/24".to_owned(),
                        "5/1/24".to_owned(),
                        131.0,
                        31.87,
                        4174.93
                    ),
                    (
                        "4/29/24".to_owned(),
                        "5/1/24".to_owned(),
                        87.0,
                        31.87,
                        2772.66
                    ),
                    (
                        "3/11/24".to_owned(),
                        "3/13/24".to_owned(),
                        38.0,
                        43.85,
                        1666.28
                    ),
                    (
                        "2/20/24".to_owned(),
                        "2/22/24".to_owned(),
                        150.0,
                        43.9822,
                        6597.27
                    )
                ],
                vec![]
            )))
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_parse_brokerage_statement() -> Result<(), String> {
        assert_eq!(
            parse_statement("data/example-divs.pdf"),
            (Ok((
                vec![],
                vec![("03/01/22".to_owned(), 698.25, 104.74)],
                vec![],
                vec![]
            )))
        );
        assert_eq!(
            parse_statement("data/example-sold-wire.pdf"),
            Ok((
                vec![],
                vec![],
                vec![(
                    "05/02/22".to_owned(),
                    "05/04/22".to_owned(),
                    -1.0,
                    43.69,
                    43.67
                )],
                vec![]
            ))
        );

        assert_eq!(
            parse_statement("data/example-sold-amd.pdf"),
            Ok((
                vec![],
                vec![],
                vec![
                    (
                        "11/10/23".to_owned(),
                        "11/14/23".to_owned(),
                        72.0,
                        118.13,
                        8505.29
                    ),
                    (
                        "11/22/23".to_owned(),
                        "11/27/23".to_owned(),
                        162.0,
                        122.4511,
                        19836.92
                    ),
                ],
                vec![]
            ))
        );

        //TODO(jczaja): Renable reinvest dividends case as soon as you get some PDFs
        //assert_eq!(
        //    parse_statement("data/example3.pdf"),
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
        //    parse_statement("data/example5.pdf"),
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
