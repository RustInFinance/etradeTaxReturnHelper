// SPDX-FileCopyrightText: 2022-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

use pdf::file::File;
use pdf::object::PageRc;
use pdf::primitive::Primitive;
use std::collections::{BTreeSet, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::str::FromStr;

use lopdf::content::Content;
use lopdf::{Dictionary as LoDictionary, Document as LoDocument, Object as LoObject, ObjectId};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

pub use crate::logging::ResultExt;

#[derive(Clone, Debug, PartialEq)]
enum StatementType {
    UnknownDocument,
    BrokerageStatement,
    AccountStatement,
    TradeConfirmation,
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
    SearchingYear,
    ProcessingYear,
    SearchingCashFlowBlock,
    SearchingTransactionEntry,
    ProcessingTransaction(TransactionType),
}

pub trait Entry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) -> Result<(), String>;
    fn get_decimal(&self) -> Option<Decimal> {
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

struct DecimalEntry {
    pub val: Decimal,
}

impl Entry for DecimalEntry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) -> Result<(), String> {
        let mystr = pstr
            .clone()
            .into_string()
            .map_err(|_| format!("Error parsing : {:#?} to Decimal", pstr))?;
        // Extracted string should have "," removed and then be parsed
        let cleaned = mystr
            .trim()
            .replace(",", "")
            .replace("(", "")
            .replace(")", "")
            .replace("$", "");
        self.val = Decimal::from_str(&cleaned)
            .map_err(|_| format!("Error parsing : {} to Decimal", mystr))?;
        log::info!("Parsed Decimal value: {}", self.val);
        Ok(())
    }
    fn get_decimal(&self) -> Option<Decimal> {
        Some(self.val)
    }
}

struct I32Entry {
    pub val: i32,
}

impl Entry for I32Entry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) -> Result<(), String> {
        let mystr = pstr
            .clone()
            .into_string()
            .map_err(|_| format!("Error parsing : {:#?} to i32", pstr))?;
        let cleaned = mystr.trim().replace(",", "");
        self.val = cleaned.parse::<i32>().or_else(|_| {
            // Handle cases where the number might be formatted as a decimal with no fractional part, e.g., "100.00"
            Decimal::from_str(&cleaned)
                .ok()
                .filter(|d| d.fract().is_zero())
                .and_then(|d| d.to_i32())
                .ok_or_else(|| format!("Error parsing : {} to i32", mystr))
        })?;
        log::info!("Parsed i32 value: {}", self.val);
        Ok(())
    }
    fn geti32(&self) -> Option<i32> {
        Some(self.val)
    }
}

struct DateEntry {
    pub val: String,
}

impl Entry for DateEntry {
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) -> Result<(), String> {
        let mystr = pstr
            .clone()
            .into_string()
            .map_err(|_| format!("Error parsing : {:#?} to Date", pstr))?;

        if chrono::NaiveDate::parse_from_str(&mystr, "%m/%d/%y").is_ok() {
            self.val = mystr;
            log::info!("Parsed date value: {}", self.val);
        }
        Ok(())
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
    fn parse(&mut self, pstr: &pdf::primitive::PdfString) -> Result<(), String> {
        self.val = pstr
            .clone()
            .into_string()
            .map_err(|_| format!("Error parsing : {:#?} to String", pstr))?;
        log::info!("Parsed String value: {}", self.val);
        Ok(())
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
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Tax Entry
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Income Entry
}

fn create_tax_parsing_sequence(sequence: &mut std::collections::VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![
            "TREASURY LIQUIDITY FUND".to_owned(),
            "INTEL CORP".to_owned(),
            "ADVANCED MICRO DEVICES".to_owned(),
        ],
    }));
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Tax Entry
}

fn create_tax_withholding_adjusted_parsing_sequence(
    sequence: &mut std::collections::VecDeque<Box<dyn Entry>>,
) {
    // Description
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![],
    }));
    // Comment
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![],
    }));
    // Money returned to tax-payer
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO }));
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
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Income Entry
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
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Income Entry
}

fn create_qualified_dividend_parsing_sequence(
    sequence: &mut std::collections::VecDeque<Box<dyn Entry>>,
) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["INTEL CORP".to_owned()],
    }));
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Income Entry
}

fn create_sold_parsing_sequence(sequence: &mut std::collections::VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["INTC".to_owned(), "DLB".to_owned()],
    })); // INTC, DLB
    sequence.push_back(Box::new(I32Entry { val: 0 })); // Quantity
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Price
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Amount Sold
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
    sequence.push_back(Box::new(I32Entry { val: 0 })); // Quantity
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Price
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Amount Sold
}

// Account statements record the company full name (e.g. "INTEL CORP", "ADVANCED MICRO DEVICES")
// while trade confirmations and G&L reports use the ticker symbol (e.g. "INTC", "AMD").
// The mapping mirrors the hard-coded company names in create_sold_2_parsing_sequence above;
// keeping these functions adjacent makes it easy to update both when adding a new company.
// Normalising here keeps the symbol representation consistent across all transaction sources
// so that matching logic downstream does not need to know both forms.
fn normalize_company_to_ticker(company: &str) -> String {
    match company {
        "INTEL CORP" => "INTC".to_owned(),
        "ADVANCED MICRO DEVICES" => "AMD".to_owned(),
        other => other.to_owned(),
    }
}

fn create_trade_parsing_sequence(sequence: &mut VecDeque<Box<dyn Entry>>) {
    sequence.push_back(Box::new(DateEntry { val: String::new() })); // Trade date
    sequence.push_back(Box::new(DateEntry { val: String::new() })); // Settlement date
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![],
    })); // MKT /
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![],
    })); // / CPT
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec![],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["SELL".to_owned(), "BUY".to_owned()],
    }));
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // Quantity
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["$".to_owned()],
    })); // $...
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // ..<price>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["PRINCIPAL".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["$".to_owned()],
    }));
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // ..<principal>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["COMMISSION".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["$".to_owned()],
    })); // $...
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // ..<commission>
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["FEE".to_owned(), "FEES".to_owned()],
    }));
    sequence.push_back(Box::new(StringEntry {
        val: String::new(),
        patterns: vec!["$".to_owned()],
    })); // $...
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // ..<fee>
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
    sequence.push_back(Box::new(DecimalEntry { val: Decimal::ZERO })); // ..<net amount>
}

fn yield_trade_confirmation_transaction(
    transaction: &mut std::slice::Iter<'_, Box<dyn Entry>>,
) -> Result<
    (
        String,
        String,
        i32,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    ),
    String,
> {
    let trade_date = transaction
        .next()
        .unwrap()
        .getdate()
        .ok_or("Error parsing trade confirmation: missing trade date")?;
    let settlement_date = transaction
        .next()
        .unwrap()
        .getdate()
        .ok_or("Error parsing trade confirmation: missing settlement date")?;

    // Skip MKT/CPT tokens.
    transaction.next();
    transaction.next();

    // Extract symbol (ticker).
    let symbol = transaction
        .next()
        .unwrap()
        .getstring()
        .map(|s| s.trim().to_uppercase())
        .filter(|s| !s.is_empty());

    // Skip SELL/BUY token.
    transaction.next();

    let quantity = transaction
        .next()
        .unwrap()
        .get_decimal()
        .ok_or("Error parsing trade confirmation: missing quantity")?
        .round()
        .to_i32()
        .ok_or("Error converting quantity to i32")?;

    transaction.next(); // $
    let price = transaction
        .next()
        .unwrap()
        .get_decimal()
        .ok_or("Error parsing trade confirmation: missing price")?;

    transaction.next(); // PRINCIPAL
    transaction.next(); // $
    let principal = transaction
        .next()
        .unwrap()
        .get_decimal()
        .ok_or("Error parsing trade confirmation: missing principal")?;

    transaction.next(); // COMMISSION
    transaction.next(); // $
    let commission = transaction
        .next()
        .unwrap()
        .get_decimal()
        .ok_or("Error parsing trade confirmation: missing commission")?;

    transaction.next(); // FEE / FEES
    transaction.next(); // $
    let fee = transaction
        .next()
        .unwrap()
        .get_decimal()
        .ok_or("Error parsing trade confirmation: missing fee")?;

    transaction.next(); // NET
    transaction.next(); // AMOUNT
    transaction.next(); // $
    let net_amount = transaction
        .next()
        .unwrap()
        .get_decimal()
        .ok_or("Error parsing trade confirmation: missing net amount")?;

    Ok((
        trade_date,
        settlement_date,
        quantity,
        price,
        principal,
        commission,
        fee,
        net_amount,
        symbol,
    ))
}

fn process_trade_confirmation_transaction(
    actual_string: &pdf::primitive::PdfString,
    processed_sequence: &mut Vec<Box<dyn Entry>>,
    sequence: &mut VecDeque<Box<dyn Entry>>,
    trades: &mut Vec<(
        String,
        String,
        i32,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    )>,
) -> Result<(), String> {
    let Some(mut obj) = sequence.pop_front() else {
        return Ok(());
    };

    obj.parse(actual_string)?;

    match obj.getstring() {
        Some(token) => {
            if obj.is_pattern() {
                processed_sequence.push(obj);
            } else if token != "$" {
                // Keep scanning input until the next anchor token appears.
                sequence.push_front(obj);
            }
        }
        None => processed_sequence.push(obj),
    }

    if sequence.is_empty() {
        let mut transaction = processed_sequence.iter();
        let trade = yield_trade_confirmation_transaction(&mut transaction)?;
        trades.push(trade);
        processed_sequence.clear();
    }

    Ok(())
}

fn resolve_lopdf_object(doc: &LoDocument, obj: &LoObject) -> Option<LoObject> {
    match obj {
        LoObject::Reference(id) => doc.get_object(*id).ok().cloned(),
        _ => Some(obj.clone()),
    }
}

fn lopdf_dict_from_object(doc: &LoDocument, obj: &LoObject) -> Option<LoDictionary> {
    resolve_lopdf_object(doc, obj)?.as_dict().ok().cloned()
}

fn decode_lopdf_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF && bytes.len() % 2 == 0 {
        let mut out = String::new();
        let mut i = 2usize;
        while i < bytes.len() {
            let v = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as u32;
            if let Some(ch) = char::from_u32(v) {
                out.push(ch);
            }
            i += 2;
        }
        return out;
    }

    let utf8 = String::from_utf8_lossy(bytes).into_owned();
    if !utf8.trim().is_empty() {
        return utf8;
    }

    bytes.iter().map(|b| *b as char).collect()
}

fn append_text_from_content(
    doc: &LoDocument,
    content_bytes: &[u8],
    resources: &LoDictionary,
    visited_forms: &mut BTreeSet<ObjectId>,
    out: &mut String,
) {
    let Ok(content) = Content::decode(content_bytes) else {
        return;
    };

    for op in &content.operations {
        match op.operator.as_str() {
            "Tj" => {
                if let Some(LoObject::String(bytes, _)) = op.operands.get(0) {
                    out.push_str(decode_lopdf_string(bytes).as_str());
                    out.push(' ');
                }
            }
            "TJ" => {
                if let Some(LoObject::Array(items)) = op.operands.get(0) {
                    for item in items {
                        if let LoObject::String(bytes, _) = item {
                            out.push_str(decode_lopdf_string(bytes).as_str());
                        }
                    }
                    out.push(' ');
                }
            }
            "'" | "\"" => {
                for operand in &op.operands {
                    if let LoObject::String(bytes, _) = operand {
                        out.push_str(decode_lopdf_string(bytes).as_str());
                        out.push(' ');
                    }
                }
            }
            "Do" => {
                let Some(LoObject::Name(xobj_name)) = op.operands.get(0) else {
                    continue;
                };
                let Some(xobj_obj) = resources.get(b"XObject").ok() else {
                    continue;
                };
                let Some(xobj_dict) = lopdf_dict_from_object(doc, xobj_obj) else {
                    continue;
                };
                let Some(target_obj) = xobj_dict.get(xobj_name).ok() else {
                    continue;
                };

                let target_id = match target_obj {
                    LoObject::Reference(id) => Some(*id),
                    _ => None,
                };
                if let Some(id) = target_id {
                    if visited_forms.contains(&id) {
                        continue;
                    }
                    visited_forms.insert(id);
                }

                let Some(resolved) = resolve_lopdf_object(doc, target_obj) else {
                    continue;
                };
                let Some(stream) = resolved.as_stream().ok() else {
                    continue;
                };

                let subtype = stream
                    .dict
                    .get(b"Subtype")
                    .ok()
                    .and_then(|o| o.as_name().ok())
                    .unwrap_or(b"");
                if subtype != b"Form" {
                    continue;
                }

                let form_resources = stream
                    .dict
                    .get(b"Resources")
                    .ok()
                    .and_then(|o| lopdf_dict_from_object(doc, o))
                    .unwrap_or_else(|| resources.clone());

                let Ok(form_content) = stream.decompressed_content() else {
                    continue;
                };

                append_text_from_content(doc, &form_content, &form_resources, visited_forms, out);
            }
            _ => {}
        }
    }
}

fn extract_text_with_lopdf(pdftoparse: &str) -> Result<String, String> {
    let doc = LoDocument::load(pdftoparse)
        .map_err(|e| format!("Unable to read PDF with low-level parser: {e}"))?;
    let mut out = String::new();

    for (_page_no, page_id) in doc.get_pages() {
        let page_obj = doc
            .get_object(page_id)
            .map_err(|e| format!("Unable to access page object: {e}"))?
            .clone();
        let page_dict = page_obj
            .as_dict()
            .map_err(|_| "Unable to decode page dictionary".to_string())?
            .clone();

        let resources = page_dict
            .get(b"Resources")
            .ok()
            .and_then(|o| lopdf_dict_from_object(&doc, o))
            .unwrap_or_default();

        let content_bytes = doc
            .get_page_content(page_id)
            .map_err(|e| format!("Unable to decode page content: {e}"))?;

        let mut visited_forms: BTreeSet<ObjectId> = BTreeSet::new();
        append_text_from_content(
            &doc,
            &content_bytes,
            &resources,
            &mut visited_forms,
            &mut out,
        );
        out.push(' ');
    }

    Ok(out)
}

fn extract_page_texts_with_lopdf(pdftoparse: &str) -> Result<Vec<String>, String> {
    let doc = LoDocument::load(pdftoparse)
        .map_err(|e| format!("Unable to read PDF with low-level parser: {e}"))?;
    let mut pages_text = vec![];

    for (_page_no, page_id) in doc.get_pages() {
        let page_obj = doc
            .get_object(page_id)
            .map_err(|e| format!("Unable to access page object: {e}"))?
            .clone();
        let page_dict = page_obj
            .as_dict()
            .map_err(|_| "Unable to decode page dictionary".to_string())?
            .clone();

        let resources = page_dict
            .get(b"Resources")
            .ok()
            .and_then(|o| lopdf_dict_from_object(&doc, o))
            .unwrap_or_default();

        let content_bytes = doc
            .get_page_content(page_id)
            .map_err(|e| format!("Unable to decode page content: {e}"))?;

        let mut page_out = String::new();
        let mut visited_forms: BTreeSet<ObjectId> = BTreeSet::new();
        append_text_from_content(
            &doc,
            &content_bytes,
            &resources,
            &mut visited_forms,
            &mut page_out,
        );

        pages_text.push(page_out);
    }

    Ok(pages_text)
}

fn hash_normalized_page_text(page_text: &str) -> u64 {
    let normalized = page_text
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized.hash(&mut hasher);
    hasher.finish()
}

fn should_skip_duplicate_multi_transaction_page(
    page_text: &str,
    parsed_trade_count: usize,
    seen_multi_transaction_page_hashes: &mut HashSet<u64>,
) -> bool {
    // Dedup applies only when a page yields multiple rows because E*TRADE can emit
    // byte-identical multi-transaction pages in more than one Trade Confirmation PDF
    // for the same trading day.
    //
    // This is expected when there were 2+ transactions on a day and bulk download was used:
    // separate confirmation documents may each repeat the same same-day summary page listing all trades from that
    // day. In that case, keeping every identical multi-row page would double count proceeds
    // and fees.
    //
    // Single-row pages are left untouched to avoid accidentally dropping legitimate trades.
    if parsed_trade_count <= 1 {
        return false;
    }

    let page_hash = hash_normalized_page_text(page_text);
    if seen_multi_transaction_page_hashes.contains(&page_hash) {
        return true;
    }

    seen_multi_transaction_page_hashes.insert(page_hash);
    false
}

fn normalize_trade_date(date_yyyy: &str) -> Result<String, String> {
    let parsed = chrono::NaiveDate::parse_from_str(date_yyyy, "%m/%d/%Y")
        .map_err(|_| format!("Unable to parse trade date: {date_yyyy}"))?;
    Ok(parsed.format("%m/%d/%y").to_string())
}

fn parse_trade_confirmation_from_text(
    extracted_text: &str,
) -> Result<
    Vec<(
        String,
        String,
        i32,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    )>,
    String,
> {
    let normalized_text = extracted_text
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");
    let upper = normalized_text.to_uppercase();

    let has_principal = upper.contains("PRINCIPAL");
    let has_net_amount = upper.contains("NET AMOUNT");

    if !has_principal || !has_net_amount {
        return Err(
            "Trade confirmation is missing required columns (PRINCIPAL, NET AMOUNT)".to_string(),
        );
    }

    // Regex with optional fee section
    let row_re = regex::Regex::new(
        r"(?s)(\d{2}/\d{2}/\d{4})\s+(\d{2}/\d{2}/\d{4})\s+(\d+)\s+([\d,]+(?:\.\d+)?)\s+Transaction\s+Type:\s*Sold.*?Principal\s*\$([\d,]+(?:\.\d+)?)\s*(?:Commission\s*\$([\d,]+(?:\.\d+)?)\s*)?(?:(?:Supplemental\s+)?Transaction\s+Fee\s*\$([\d,]+(?:\.\d+)?)\s*)?Net\s+Amount\s*\$([\d,]+(?:\.\d+)?)",
    )
    .map_err(|_| "Unable to create regex for trade confirmation row parsing".to_string())?;

    let parse_money = |s: &str| -> Result<Decimal, String> {
        let cleaned = s.replace(',', "");
        Decimal::from_str(&cleaned).map_err(|_| format!("Unable to parse money value: {s}"))
    };

    let mut trades: Vec<(
        String,
        String,
        i32,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    )> = vec![];
    for cap in row_re.captures_iter(&normalized_text) {
        let trade_date = normalize_trade_date(&cap[1])?;
        let settlement_date = normalize_trade_date(&cap[2])?;
        let quantity = cap[3]
            .parse::<i32>()
            .map_err(|_| format!("Unable to parse quantity: {}", &cap[3]))?;
        let price = parse_money(&cap[4])?;
        let principal = parse_money(&cap[5])?;
        let commission = if let Some(c) = cap.get(6) {
            parse_money(c.as_str())?
        } else {
            Decimal::ZERO
        };
        let fee = if let Some(f) = cap.get(7) {
            parse_money(f.as_str())?
        } else {
            Decimal::ZERO
        };
        let net_amount = parse_money(&cap[8])?;

        // Sanity check: principal - commission - fee should equal net_amount
        let calculated_net = principal - commission - fee;
        let delta = (calculated_net - net_amount).abs();
        log::info!(
            "Parsed trade - Trade Date: {}, Settlement Date: {}, Quantity: {}, Price: {}, Principal: {}, Commission: {}, Fee: {}, Net Amount: {}",
            trade_date, settlement_date, quantity, price, principal, commission, fee, net_amount
        );
        if delta != Decimal::ZERO {
            return Err(format!(
                "Trade confirmation sanity check failed: principal ({}) - commission ({}) - fee ({}) = {} but net amount is {} (delta: {})",
                principal, commission, fee, calculated_net, net_amount, delta
            ));
        }

        trades.push((
            trade_date,
            settlement_date,
            quantity,
            price,
            principal,
            commission,
            fee,
            net_amount,
            None, // Symbol not available in regex fallback
        ));
    }

    if trades.is_empty() {
        return Err(
            "Trade confirmation detected, but no complete transaction rows were parsed".to_string(),
        );
    }

    Ok(trades)
}

fn parse_trade_confirmation_lopdf(
    pdftoparse: &str,
    seen_multi_transaction_page_hashes: &mut HashSet<u64>,
) -> Result<
    (
        Vec<(String, Decimal, Decimal)>,
        Vec<(String, Decimal, Decimal, Option<String>)>,
        Vec<(String, String, i32, Decimal, Decimal, Option<String>)>,
        Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )>,
    ),
    String,
> {
    let mut trades: Vec<(
        String,
        String,
        i32,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    )> = vec![];
    let page_texts = extract_page_texts_with_lopdf(pdftoparse)?;
    let mut found_trade_rows_on_any_page = false;
    let mut skipped_duplicate_multi_transaction_page = false;

    for (page_idx, page_text) in page_texts.iter().enumerate() {
        match parse_trade_confirmation_from_text(page_text) {
            Ok(mut page_trades) => {
                found_trade_rows_on_any_page = true;
                // Page-wise dedup prevents double counting when separately selected Trade
                // Confirmation PDFs repeat the same same-day multi-transaction page.
                // This duplication is expected for days with multiple trades and is not treated
                // as distinct economic activity.
                if should_skip_duplicate_multi_transaction_page(
                    page_text,
                    page_trades.len(),
                    seen_multi_transaction_page_hashes,
                ) {
                    skipped_duplicate_multi_transaction_page = true;
                    log::warn!(
                        "Skipping duplicate multi-transaction trade confirmation page {} from {}",
                        page_idx + 1,
                        pdftoparse
                    );
                    continue;
                }
                trades.append(&mut page_trades);
            }
            Err(e)
                if e.contains("missing required columns")
                    || e.contains("no complete transaction rows were parsed") =>
            {
                continue;
            }
            Err(e) => {
                return Err(format!(
                    "Unable to parse trade confirmation page {}: {}",
                    page_idx + 1,
                    e
                ));
            }
        }
    }

    if trades.is_empty() {
        if found_trade_rows_on_any_page && skipped_duplicate_multi_transaction_page {
            log::info!(
                "Trade confirmation {} was fully skipped because all parsed rows were duplicates",
                pdftoparse
            );
            return Ok((vec![], vec![], vec![], vec![]));
        }

        return Err(
            "Trade confirmation detected, but no complete transaction rows were parsed".to_string(),
        );
    }

    Ok((vec![], vec![], vec![], trades))
}

fn parse_trade_confirmation<'a, I>(
    first_page: PageRc,
    pages_iter: I,
) -> Result<
    (
        Vec<(String, Decimal, Decimal)>,
        Vec<(String, Decimal, Decimal, Option<String>)>,
        Vec<(String, String, i32, Decimal, Decimal, Option<String>)>,
        Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )>,
    ),
    String,
>
where
    I: Iterator<Item = Result<PageRc, pdf::error::PdfError>>,
{
    let interests_transactions: Vec<(String, Decimal, Decimal)> = vec![];
    let div_transactions: Vec<(String, Decimal, Decimal, Option<String>)> = vec![];
    let sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> = vec![];
    let mut trades: Vec<(
        String,
        String,
        i32,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    )> = vec![];

    let full_date_pattern =
        regex::Regex::new(r"^(0?[1-9]|1[012])/(0?[1-9]|[12][0-9]|3[01])/\d{2}$")
            .map_err(|_| "Unable to create regular expression to parse trade confirmation date")?;

    let mut sequence: VecDeque<Box<dyn Entry>> = VecDeque::new();
    let mut processed_sequence: Vec<Box<dyn Entry>> = vec![];
    let mut found_principal = false;
    let mut found_commission = false;
    let mut found_fee = false;
    let mut found_net = false;
    let mut found_amount = false;

    let mut pages = vec![first_page];
    for page in pages_iter {
        pages.push(page.map_err(|_| "Unable to read PDF page when parsing trade confirmation")?);
    }

    for page in pages {
        let Some(contents) = page.contents.as_ref() else {
            continue;
        };

        for op in contents.operations.iter() {
            match op.operator.as_ref() {
                "Tj" => {
                    if let Some(Primitive::String(actual_string)) = op.operands.get(0) {
                        let raw_string = actual_string.clone().into_string();
                        let rust_string = if let Ok(r) = raw_string {
                            r.trim().to_uppercase()
                        } else {
                            "".to_owned()
                        };
                        if rust_string.is_empty() {
                            continue;
                        }

                        if rust_string == "PRINCIPAL" {
                            found_principal = true;
                        } else if rust_string == "COMMISSION" {
                            found_commission = true;
                        } else if rust_string == "FEE" || rust_string == "FEES" {
                            found_fee = true;
                        } else if rust_string == "NET" {
                            found_net = true;
                        } else if rust_string == "AMOUNT" {
                            found_amount = true;
                        }

                        if sequence.is_empty() && full_date_pattern.is_match(rust_string.as_str()) {
                            create_trade_parsing_sequence(&mut sequence);
                        }

                        if !sequence.is_empty() {
                            process_trade_confirmation_transaction(
                                actual_string,
                                &mut processed_sequence,
                                &mut sequence,
                                &mut trades,
                            )?;
                        }
                    }
                }
                "TJ" => {
                    if let Some(Primitive::Array(items)) = op.operands.get(0) {
                        for item in items {
                            if let Primitive::String(actual_string) = item {
                                let raw_string = actual_string.clone().into_string();
                                let rust_string = if let Ok(r) = raw_string {
                                    r.trim().to_uppercase()
                                } else {
                                    "".to_owned()
                                };
                                if rust_string.is_empty() {
                                    continue;
                                }

                                if rust_string == "PRINCIPAL" {
                                    found_principal = true;
                                } else if rust_string == "COMMISSION" {
                                    found_commission = true;
                                } else if rust_string == "FEE" || rust_string == "FEES" {
                                    found_fee = true;
                                } else if rust_string == "NET" {
                                    found_net = true;
                                } else if rust_string == "AMOUNT" {
                                    found_amount = true;
                                }

                                if sequence.is_empty()
                                    && full_date_pattern.is_match(rust_string.as_str())
                                {
                                    create_trade_parsing_sequence(&mut sequence);
                                }

                                if !sequence.is_empty() {
                                    process_trade_confirmation_transaction(
                                        actual_string,
                                        &mut processed_sequence,
                                        &mut sequence,
                                        &mut trades,
                                    )?;
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    if !found_principal || !found_commission || !found_fee || !found_net || !found_amount {
        return Err("Trade confirmation is missing required columns (PRINCIPAL, COMMISSION, FEE/FEES, NET, AMOUNT)".to_string());
    }

    if trades.is_empty() {
        return Err(
            "Trade confirmation detected, but no complete transaction rows were parsed".to_string(),
        );
    }

    Ok((
        interests_transactions,
        div_transactions,
        sold_transactions,
        trades,
    ))
}

fn yield_sold_transaction(
    transaction: &mut std::slice::Iter<'_, Box<dyn Entry>>,
    transaction_dates: &mut Vec<String>,
) -> Option<(String, String, i32, Decimal, Decimal, Option<String>)> {
    let symbol = transaction
        .next()
        .unwrap()
        .getstring()
        .expect_and_log("Processing of Sold transaction went wrong");
    let quantity = transaction
        .next()
        .unwrap()
        .geti32()
        .expect_and_log("Processing of Sold transaction went wrong");
    let price = transaction
        .next()
        .unwrap()
        .get_decimal()
        .expect_and_log("Processing of Sold transaction went wrong");
    let amount_sold = transaction
        .next()
        .unwrap()
        .get_decimal()
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

    Some((
        trade_date,
        settlement_date,
        quantity,
        price,
        amount_sold,
        Some(normalize_company_to_ticker(&symbol)),
    ))
}

/// Recognize whether PDF document is of Brokerage Statement type (old e-trade type of PDF
/// document) or maybe Single account statment (newer e-trade/morgan stanley type of document)
fn recognize_statement(page: PageRc, pdftoparse: &str) -> Result<StatementType, String> {
    log::info!("Starting to recognize PDF document type");
    // Heuristic: Common clause in trade confirmations. Lead text varies based on whether there's single or multiple transactions in confirmation
    let confirmation_clause_common = "confirmed in accordance with the information provided on the Conditions and Disclosures page";
    let mut text_acc_raw = String::new();

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
                                    let raw_trimmed = if let Ok(r) = raw_string {
                                        r.trim().to_owned()
                                    } else {
                                        "".to_owned()
                                    };
                                    let rust_string = raw_trimmed.to_uppercase();
                                    if !rust_string.is_empty() {
                                        if !text_acc_raw.is_empty() {
                                            text_acc_raw.push(' ');
                                        }
                                        text_acc_raw.push_str(raw_trimmed.as_str());
                                        if text_acc_raw.contains(confirmation_clause_common) {
                                            statement_type = StatementType::TradeConfirmation;
                                            log::info!("PDF parser recognized Trade Confirmation document by legal confirmation clause");
                                            return Ok(());
                                        }
                                    }
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
                            let raw_trimmed = if let Ok(r) = raw_string {
                                r.trim().to_owned()
                            } else {
                                "".to_owned()
                            };
                            let rust_string = raw_trimmed.to_uppercase();

                            if !rust_string.is_empty() {
                                if !text_acc_raw.is_empty() {
                                    text_acc_raw.push(' ');
                                }
                                text_acc_raw.push_str(raw_trimmed.as_str());
                                if text_acc_raw.contains(confirmation_clause_common) {
                                    statement_type = StatementType::TradeConfirmation;
                                    log::info!("PDF parser recognized Trade Confirmation document by legal confirmation clause");
                                    return Ok(());
                                }
                            }

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

    if statement_type == StatementType::UnknownDocument {
        if let Ok(extracted_text) = extract_text_with_lopdf(pdftoparse) {
            if extracted_text.contains(confirmation_clause_common) {
                log::info!("PDF parser recognized Trade Confirmation document by legal confirmation clause (lopdf fallback)");
                return Ok(StatementType::TradeConfirmation);
            }
            if extracted_text.to_uppercase().contains("CLIENT STATEMENT") {
                log::info!("PDF parser recognized Account Statement document by finding CLIENT STATEMENT (lopdf fallback)");
                return Ok(StatementType::AccountStatement);
            }
        }
    }

    Ok(statement_type)
}

fn process_transaction(
    interests_transactions: &mut Vec<(String, Decimal, Decimal)>,
    div_transactions: &mut Vec<(String, Decimal, Decimal, Option<String>)>,
    sold_transactions: &mut Vec<(String, String, i32, Decimal, Decimal, Option<String>)>,
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
        // attach only i32 and Decimal elements to
        // processed queue
        Some(mut obj) => {
            obj.parse(actual_string)?;
            // attach to sequence the same string parser if pattern is not met
            match obj.getstring() {
                Some(token) => {
                    let support_companies = vec![
                        "TREASURY LIQUIDITY FUND".to_owned(),
                        "INTEL CORP".to_owned(),
                        "ADVANCED MICRO DEVICES".to_owned(),
                        "INTEREST ADJUSTMENT".to_owned(),
                    ];
                    if obj.is_pattern() == true {
                        if support_companies.contains(&token) == true {
                            processed_sequence.push(obj);
                        }
                    } else {
                        if token != "$" {
                            sequence.push_front(obj);
                        }
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
                        let _symbol = transaction
                            .next()
                            .unwrap()
                            .getstring()
                            .expect_and_log("Processing of Tax transaction went wrong");
                        // Ok we assume here that taxation of transaction appears later in document
                        // than actual transaction that is a subject to taxation
                        let tax_us = transaction
                            .next()
                            .unwrap()
                            .get_decimal()
                            .ok_or("Processing of Tax transaction went wrong")?;

                        // Here we just go through registered transactions and pick the one where
                        // income is higher than tax and apply tax value and where tax was not yet
                        // applied
                        let mut interests_as_div: Vec<(
                            &mut String,
                            &mut Decimal,
                            &mut Decimal,
                            Option<String>,
                        )> = interests_transactions
                            .iter_mut()
                            .map(|x| (&mut x.0, &mut x.1, &mut x.2, None))
                            .collect();
                        let mut div_as_ref: Vec<(
                            &mut String,
                            &mut Decimal,
                            &mut Decimal,
                            Option<String>,
                        )> = div_transactions
                            .iter_mut()
                            .map(|x| (&mut x.0, &mut x.1, &mut x.2, x.3.clone()))
                            .collect();

                        let subject_to_tax = div_as_ref
                            .iter_mut()
                            .chain(interests_as_div.iter_mut())
                            .find(|x| *x.1 > tax_us && *x.2 == Decimal::ZERO)
                            .ok_or("Error: Unable to find transaction that was taxed")?;
                        log::info!("Tax: {tax_us} was applied to {subject_to_tax:?}");
                        *subject_to_tax.2 = tax_us;
                        log::info!("Completed parsing Tax transaction");
                    }
                    TransactionType::Interests => {
                        let _symbol = transaction
                            .next()
                            .unwrap()
                            .getstring()
                            .expect_and_log("Processing of Interests transaction went wrong");
                        let gross_us = transaction
                            .next()
                            .unwrap()
                            .get_decimal()
                            .ok_or("Processing of Interests transaction went wrong")?;

                        interests_transactions.push((
                            transaction_dates
                                .pop()
                                .ok_or("Error: missing transaction dates when parsing")?,
                            gross_us,
                            Decimal::ZERO, // No tax info yet. It may be added later in Tax section
                        ));
                        log::info!("Completed parsing Interests transaction");
                    }
                    TransactionType::Dividends => {
                        let symbol = transaction
                            .next()
                            .unwrap()
                            .getstring()
                            .expect_and_log("Processing of Dividend transaction went wrong");
                        let gross_us = transaction
                            .next()
                            .unwrap()
                            .get_decimal()
                            .ok_or("Processing of Dividend transaction went wrong")?;

                        div_transactions.push((
                            transaction_dates
                                .pop()
                                .ok_or("Error: missing transaction dates when parsing")?,
                            gross_us,
                            Decimal::ZERO, // No tax info yet. It will be added later in Tax section
                            Some(normalize_company_to_ticker(&symbol)),
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
    } else if candidate_string == "TAX WITHHOLDING ADJ" {
        create_tax_withholding_adjusted_parsing_sequence(sequence);
        state = ParserState::ProcessingTransaction(TransactionType::Dividends);
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

/// Get last two digits of year from pattern like:  "31, 2023)"
fn yield_year(rust_string: &str) -> Option<String> {
    let re = regex::Regex::new(r"\b\d{4}\b")
        .expect("Unable to create regular expression to capture fiscal year");
    let maybe = re.find(rust_string);
    if let Some(year) = maybe {
        Some(year.as_str()[year.len() - 2..].to_string())
    } else {
        None
    }
}

/// Parse borkerage statement document type
fn parse_account_statement<'a, I>(
    pages_iter: I,
) -> Result<
    (
        Vec<(String, Decimal, Decimal)>,
        Vec<(String, Decimal, Decimal, Option<String>)>,
        Vec<(String, String, i32, Decimal, Decimal, Option<String>)>,
        Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )>,
    ),
    String,
>
where
    I: Iterator<Item = Result<PageRc, pdf::error::PdfError>>,
{
    let mut interests_transactions: Vec<(String, Decimal, Decimal)> = vec![];
    let mut div_transactions: Vec<(String, Decimal, Decimal, Option<String>)> = vec![];
    let mut sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
        vec![];
    let trades: Vec<(
        String,
        String,
        i32,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    )> = vec![];
    let mut state = ParserState::SearchingYear;
    let mut sequence: VecDeque<Box<dyn Entry>> = VecDeque::new();
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
                                        ParserState::SearchingYear => {
                                            // Pattern to match "For the Period"
                                            let date_pattern = regex::Regex::new(r"(?i)For the Period").map_err(|_| "Unable to create regular expression to capture fiscal year")?;

                                            if date_pattern.find(rust_string.as_str()).is_some()
                                                && year.is_none()
                                            {
                                                log::info!("Found pattern: \"For the Period\". Starting to parsing year");
                                                state = ParserState::ProcessingYear;
                                            }
                                        }
                                        ParserState::ProcessingYear => {
                                            log::trace!("Parsing year. Token: {rust_string}");
                                            year = yield_year(&rust_string);
                                            if year.is_some() {
                                                log::info!("Parsed year: {year:?}");
                                                state = ParserState::SearchingCashFlowBlock;
                                            }
                                        }
                                        ParserState::SearchingCashFlowBlock => {
                                            // When we find "CASH FLOW ACTIVITY BY DATE" then
                                            // it is a starting point of transactions we are
                                            // interested in
                                            if rust_string == "CASH FLOW ACTIVITY BY DATE" {
                                                state = ParserState::SearchingTransactionEntry;
                                                log::info!("Parsing account statement: \"CASH FLOW ACTIVITY BY DATE\" detected. Start to parse transactions");
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
///        transaction date, gross_us, tax_us, company
///  Sold stock transaction is :
///     (trade_date, settlement_date, quantity, price, amount_sold, company)
pub(crate) fn parse_statement_with_seen_pages(
    pdftoparse: &str,
    seen_multi_transaction_page_hashes: &mut HashSet<u64>,
) -> Result<
    (
        Vec<(String, Decimal, Decimal)>,
        Vec<(String, Decimal, Decimal, Option<String>)>,
        Vec<(String, String, i32, Decimal, Decimal, Option<String>)>,
        Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )>,
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

    let document_type = recognize_statement(first_page.clone(), pdftoparse)?;

    let (interests_transactions, div_transactions, sold_transactions, trades) = match document_type
    {
        StatementType::UnknownDocument => {
            log::info!("Processing unknown document PDF");
            return Err(format!("Unsupported PDF document type: {pdftoparse}"));
        }
        StatementType::BrokerageStatement => {
            log::info!("Processing brokerage statement PDF");
            return Err(format!("Processing brokerage statement PDF is unsupported: document type: {pdftoparse}.To have it supported please use release 0.7.4 "));
        }
        StatementType::AccountStatement => {
            log::info!("Processing Account statement PDF");
            parse_account_statement(pdffile_iter)?
        }
        StatementType::TradeConfirmation => {
            log::info!("Processing Trade Confirmation PDF");
            match parse_trade_confirmation_lopdf(pdftoparse, seen_multi_transaction_page_hashes) {
                Ok(parsed) => parsed,
                Err(e) => {
                    log::warn!("Low-level trade confirmation parser failed ({e}). Falling back to legacy parser");
                    parse_trade_confirmation(first_page, pdffile_iter)
                        .map_err(|legacy_err| format!("{} [file: {}]", legacy_err, pdftoparse))?
                }
            }
        }
    };

    Ok((
        interests_transactions,
        div_transactions,
        sold_transactions,
        trades,
    ))
}

pub fn parse_statement(
    pdftoparse: &str,
) -> Result<
    (
        Vec<(String, Decimal, Decimal)>,
        Vec<(String, Decimal, Decimal, Option<String>)>,
        Vec<(String, String, i32, Decimal, Decimal, Option<String>)>,
        Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )>,
    ),
    String,
> {
    let mut seen_multi_transaction_page_hashes = HashSet::new();
    parse_statement_with_seen_pages(pdftoparse, &mut seen_multi_transaction_page_hashes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::dec;

    #[test]
    fn test_parser() -> Result<(), String> {
        // quantity
        let data: Vec<u8> = vec!['1' as u8];
        let mut i = I32Entry { val: 0 };
        i.parse(&pdf::primitive::PdfString::new(data))?;
        assert_eq!(i.geti32(), Some(1));

        // price
        let data: Vec<u8> = vec![
            '2' as u8, '8' as u8, '.' as u8, '2' as u8, '0' as u8, '3' as u8, '5' as u8,
        ];
        let mut f = DecimalEntry { val: Decimal::ZERO };
        f.parse(&pdf::primitive::PdfString::new(data))?;
        assert_eq!(f.get_decimal(), Some(dec!(28.2035)));

        // amount
        let data: Vec<u8> = vec![
            '4' as u8, ',' as u8, '8' as u8, '7' as u8, '7' as u8, '.' as u8, '3' as u8, '6' as u8,
        ];
        let mut f = DecimalEntry { val: Decimal::ZERO };
        f.parse(&pdf::primitive::PdfString::new(data))?;
        assert_eq!(f.get_decimal(), Some(dec!(4877.36)));

        let data: Vec<u8> = vec![
            '(' as u8, '5' as u8, '7' as u8, '.' as u8, '9' as u8, '8' as u8, ')' as u8,
        ];
        let mut f = DecimalEntry { val: Decimal::ZERO };
        f.parse(&pdf::primitive::PdfString::new(data))?;
        assert_eq!(f.get_decimal(), Some(dec!(57.98)));

        let data: Vec<u8> = vec!['$' as u8, '1' as u8, '.' as u8, '2' as u8, '2' as u8];
        let mut f = DecimalEntry { val: Decimal::ZERO };
        f.parse(&pdf::primitive::PdfString::new(data))?;
        assert_eq!(f.get_decimal(), Some(dec!(1.22)));

        let data: Vec<u8> = vec![
            '8' as u8, '2' as u8, '.' as u8, '0' as u8, '0' as u8, '0' as u8,
        ];
        let mut f = DecimalEntry { val: Decimal::ZERO };
        f.parse(&pdf::primitive::PdfString::new(data))?;
        assert_eq!(f.get_decimal(), Some(dec!(82.000)));

        // company code
        let data: Vec<u8> = vec!['D' as u8, 'L' as u8, 'B' as u8];
        let mut s = StringEntry {
            val: String::new(),
            patterns: vec!["INTC".to_owned(), "DLB".to_owned()],
        };
        s.parse(&pdf::primitive::PdfString::new(data))?;
        assert!(s.is_pattern());

        // unimportant string
        let data: Vec<u8> = vec!['K' as u8, 'L' as u8, 'M' as u8];
        let mut s = StringEntry {
            val: String::new(),
            patterns: vec![],
        };
        s.parse(&pdf::primitive::PdfString::new(data))?;
        assert!(s.is_pattern());
        Ok(())
    }

    #[test]
    fn test_trade_confirmation_transaction_extraction() -> Result<(), String> {
        type ParsedTrade = (
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        );

        let tokens = vec![
            "12/02/25",
            "12/05/25",
            "1234",
            "5678",
            "INTC",
            "SELL",
            "82",
            "$",
            "28.2035",
            "PRINCIPAL",
            "$",
            "2312.69",
            "COMMISSION",
            "$",
            "0.00",
            "FEE",
            "$",
            "1.22",
            "NET",
            "AMOUNT",
            "$",
            "2311.47",
        ];

        let mut sequence: std::collections::VecDeque<Box<dyn Entry>> =
            std::collections::VecDeque::new();
        create_trade_parsing_sequence(&mut sequence);
        let mut processed_sequence: Vec<Box<dyn Entry>> = vec![];
        let mut trades: Vec<ParsedTrade> = vec![];

        for token in tokens {
            let data: Vec<u8> = token.as_bytes().to_vec();
            process_trade_confirmation_transaction(
                &pdf::primitive::PdfString::new(data),
                &mut processed_sequence,
                &mut sequence,
                &mut trades,
            )?;
        }

        assert_eq!(trades.len(), 1);
        assert_eq!(
            trades[0],
            (
                "12/02/25".to_owned(),
                "12/05/25".to_owned(),
                82,
                dec!(28.2035),
                dec!(2312.69),
                Decimal::ZERO,
                dec!(1.22),
                dec!(2311.47),
                Some("INTC".to_owned()),
            )
        );

        Ok(())
    }

    #[test]
    fn test_skip_duplicate_multi_transaction_page_by_hash() -> Result<(), String> {
        let duplicated_multi_transaction_page = "
            12/02/2025 12/05/2025 82 28.2035 Transaction Type: Sold
            Principal $2312.69 Commission $0.00 Transaction Fee $1.22 Net Amount $2311.47
            12/02/2025 12/05/2025 41 28.0000 Transaction Type: Sold
            Principal $1148.00 Commission $0.00 Transaction Fee $0.50 Net Amount $1147.50
        ";

        let parsed_once = parse_trade_confirmation_from_text(duplicated_multi_transaction_page)?;
        assert_eq!(parsed_once.len(), 2);

        let mut seen_hashes = HashSet::new();

        let first_time_skipped = should_skip_duplicate_multi_transaction_page(
            duplicated_multi_transaction_page,
            parsed_once.len(),
            &mut seen_hashes,
        );
        assert!(!first_time_skipped);

        let parsed_twice = parse_trade_confirmation_from_text(duplicated_multi_transaction_page)?;
        assert_eq!(parsed_twice.len(), 2);

        let second_time_skipped = should_skip_duplicate_multi_transaction_page(
            duplicated_multi_transaction_page,
            parsed_twice.len(),
            &mut seen_hashes,
        );
        assert!(second_time_skipped);

        Ok(())
    }

    #[test]
    fn test_do_not_skip_duplicate_single_transaction_page_by_hash() -> Result<(), String> {
        let duplicated_single_transaction_page = "
            12/02/2025 12/05/2025 82 28.2035 Transaction Type: Sold
            Principal $2312.69 Commission $0.00 Transaction Fee $1.22 Net Amount $2311.47
        ";

        let parsed_once = parse_trade_confirmation_from_text(duplicated_single_transaction_page)?;
        assert_eq!(parsed_once.len(), 1);

        let mut seen_hashes = HashSet::new();

        let first_time_skipped = should_skip_duplicate_multi_transaction_page(
            duplicated_single_transaction_page,
            parsed_once.len(),
            &mut seen_hashes,
        );
        assert!(!first_time_skipped);

        let parsed_twice = parse_trade_confirmation_from_text(duplicated_single_transaction_page)?;
        assert_eq!(parsed_twice.len(), 1);

        let second_time_skipped = should_skip_duplicate_multi_transaction_page(
            duplicated_single_transaction_page,
            parsed_twice.len(),
            &mut seen_hashes,
        );
        assert!(!second_time_skipped);

        Ok(())
    }

    #[test]
    fn test_transaction_validation() -> Result<(), String> {
        let mut transaction_dates: Vec<String> =
            vec!["11/29/22".to_string(), "12/01/22".to_string()];
        let processed_sequence: Vec<Box<dyn Entry>> = vec![
            Box::new(StringEntry {
                val: String::new(),
                patterns: vec!["INTC".to_owned(), "DLB".to_owned()],
            }),
            Box::new(I32Entry { val: 42 }),
            Box::new(DecimalEntry { val: dec!(28.8400) }),
            Box::new(DecimalEntry { val: dec!(1210.83) }),
        ];

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
        let processed_sequence: Vec<Box<dyn Entry>> = vec![
            Box::new(StringEntry {
                val: String::new(),
                patterns: vec!["INTC".to_owned(), "DLB".to_owned()],
            }),
            Box::new(I32Entry { val: 42 }),
            Box::new(DecimalEntry { val: dec!(28.8400) }),
            Box::new(DecimalEntry { val: dec!(1210.83) }),
        ];

        yield_sold_transaction(&mut processed_sequence.iter(), &mut transaction_dates)
            .ok_or("Parsing error".to_string())?;
        Ok(())
    }

    #[test]
    fn test_unsettled_transaction_validation() -> Result<(), String> {
        let mut transaction_dates: Vec<String> = vec!["11/29/22".to_string()];
        let processed_sequence: Vec<Box<dyn Entry>> = vec![
            Box::new(StringEntry {
                val: String::new(),
                patterns: vec!["INTC".to_owned(), "DLB".to_owned()],
            }),
            Box::new(I32Entry { val: 42 }),
            Box::new(DecimalEntry { val: dec!(28.8400) }),
            Box::new(DecimalEntry { val: dec!(1210.83) }),
        ];

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
        let rust_string = "31, 2023";
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

        let document_type = recognize_statement(first_page, pdftoparse)?;

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

        let document_type = recognize_statement(first_page, pdftoparse)?;

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

        let document_type = recognize_statement(first_page, pdftoparse)?;

        assert_eq!(document_type, StatementType::UnknownDocument);

        Ok(())
    }

    #[test]
    #[ignore]
    fn test_account_statement() -> Result<(), String> {
        assert_eq!(
            parse_statement("data/MS_ClientStatements_6557_202312.pdf"),
            (Ok((
                vec![("12/1/23".to_owned(), dec!(1.22), dec!(0.00))],
                vec![(
                    "12/1/23".to_owned(),
                    dec!(386.50),
                    dec!(57.98),
                    Some("INTC".to_string())
                ),],
                vec![(
                    "12/21/23".to_owned(),
                    "12/26/23".to_owned(),
                    82,
                    dec!(46.45),
                    dec!(3808.86),
                    Some("INTC".to_string())
                )],
                vec![]
            )))
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_account_statement_tax_on_interests() -> Result<(), String> {
        assert_eq!(
            parse_statement("data/example_interests_taxing.pdf"),
            (Ok((
                vec![("1/2/24".to_owned(), dec!(0.92), dec!(0.22))],
                vec![],
                vec![],
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
                    ("12/2/24".to_owned(), dec!(4.88), dec!(0.00)),
                    ("10/1/24".to_owned(), dec!(24.91), dec!(0.00)),
                    ("11/1/24".to_owned(), dec!(25.09), dec!(0.00)),
                    ("9/3/24".to_owned(), dec!(23.65), dec!(0.00)), // Interest rates
                    ("8/1/24".to_owned(), dec!(4.34), dec!(0.00)),
                    ("7/1/24".to_owned(), dec!(3.72), dec!(0.00)),
                    ("6/3/24".to_owned(), dec!(13.31), dec!(0.00)),
                    ("5/1/24".to_owned(), dec!(0.62), dec!(0.00)),
                    ("4/1/24".to_owned(), dec!(1.16), dec!(0.00)),
                    ("1/2/24".to_owned(), dec!(0.49), dec!(0.00))
                ],
                vec![
                    (
                        "6/3/24".to_owned(),
                        dec!(57.25),
                        dec!(8.59),
                        Some("INTC".to_owned())
                    ), // Dividends date, gross, tax_us
                    (
                        "3/1/24".to_owned(),
                        dec!(380.25),
                        dec!(57.04),
                        Some("INTC".to_owned())
                    )
                ],
                vec![
                    (
                        "12/4/24".to_owned(),
                        "12/5/24".to_owned(),
                        30,
                        dec!(22.5),
                        dec!(674.98),
                        Some("INTC".to_string())
                    ),
                    (
                        "12/5/24".to_owned(),
                        "12/6/24".to_owned(),
                        55,
                        dec!(21.96),
                        dec!(1207.76),
                        Some("INTC".to_string())
                    ),
                    (
                        "11/1/24".to_owned(),
                        "11/4/24".to_owned(),
                        15,
                        dec!(23.32),
                        dec!(349.79),
                        Some("INTC".to_string())
                    ),
                    (
                        "9/3/24".to_owned(),
                        "9/4/24".to_owned(),
                        17,
                        dec!(21.53),
                        dec!(365.99),
                        Some("INTC".to_string())
                    ), // Sold
                    (
                        "9/9/24".to_owned(),
                        "9/10/24".to_owned(),
                        14,
                        dec!(18.98),
                        dec!(265.71),
                        Some("INTC".to_string())
                    ),
                    (
                        "8/5/24".to_owned(),
                        "8/6/24".to_owned(),
                        14,
                        dec!(20.21),
                        dec!(282.93),
                        Some("INTC".to_string())
                    ),
                    (
                        "8/20/24".to_owned(),
                        "8/21/24".to_owned(),
                        328,
                        dec!(21.0247),
                        dec!(6895.89),
                        Some("INTC".to_string())
                    ),
                    (
                        "7/31/24".to_owned(),
                        "8/1/24".to_owned(),
                        151,
                        dec!(30.44),
                        dec!(4596.31),
                        Some("INTC".to_string())
                    ),
                    (
                        "6/3/24".to_owned(),
                        "6/4/24".to_owned(),
                        14,
                        dec!(31.04),
                        dec!(434.54),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/1/24".to_owned(),
                        "5/3/24".to_owned(),
                        126,
                        dec!(30.14),
                        dec!(3797.6),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/1/24".to_owned(),
                        "5/3/24".to_owned(),
                        124,
                        dec!(30.14),
                        dec!(3737.33),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/1/24".to_owned(),
                        "5/3/24".to_owned(),
                        89,
                        dec!(30.6116),
                        dec!(2724.4),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/2/24".to_owned(),
                        "5/6/24".to_owned(),
                        182,
                        dec!(30.56),
                        dec!(5561.87),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        440,
                        dec!(30.835),
                        dec!(13567.29),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        198,
                        dec!(30.835),
                        dec!(6105.28),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        146,
                        dec!(30.8603),
                        dec!(4505.56),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        145,
                        dec!(30.8626),
                        dec!(4475.04),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/3/24".to_owned(),
                        "5/7/24".to_owned(),
                        75,
                        dec!(30.815),
                        dec!(2311.11),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/6/24".to_owned(),
                        "5/8/24".to_owned(),
                        458,
                        dec!(31.11),
                        dec!(14248.26),
                        Some("INTC".to_string())
                    ),
                    (
                        "5/31/24".to_owned(),
                        "6/3/24".to_owned(),
                        18,
                        dec!(30.22),
                        dec!(543.94),
                        Some("INTC".to_string())
                    ),
                    (
                        "4/3/24".to_owned(),
                        "4/5/24".to_owned(),
                        31,
                        dec!(40.625),
                        dec!(1259.36),
                        Some("INTC".to_string())
                    ),
                    (
                        "4/11/24".to_owned(),
                        "4/15/24".to_owned(),
                        209,
                        dec!(37.44),
                        dec!(7824.89),
                        Some("INTC".to_string())
                    ),
                    (
                        "4/11/24".to_owned(),
                        "4/15/24".to_owned(),
                        190,
                        dec!(37.44),
                        dec!(7113.54),
                        Some("INTC".to_string())
                    ),
                    (
                        "4/16/24".to_owned(),
                        "4/18/24".to_owned(),
                        310,
                        dec!(36.27),
                        dec!(11243.61),
                        Some("INTC".to_string())
                    ),
                    (
                        "4/29/24".to_owned(),
                        "5/1/24".to_owned(),
                        153,
                        dec!(31.87),
                        dec!(4876.07),
                        Some("INTC".to_string())
                    ),
                    (
                        "4/29/24".to_owned(),
                        "5/1/24".to_owned(),
                        131,
                        dec!(31.87),
                        dec!(4174.93),
                        Some("INTC".to_string())
                    ),
                    (
                        "4/29/24".to_owned(),
                        "5/1/24".to_owned(),
                        87,
                        dec!(31.87),
                        dec!(2772.66),
                        Some("INTC".to_string())
                    ),
                    (
                        "3/11/24".to_owned(),
                        "3/13/24".to_owned(),
                        38,
                        dec!(43.85),
                        dec!(1666.28),
                        Some("INTC".to_string())
                    ),
                    (
                        "2/20/24".to_owned(),
                        "2/22/24".to_owned(),
                        150,
                        dec!(43.9822),
                        dec!(6597.27),
                        Some("INTC".to_string())
                    )
                ],
                vec![]
            )))
        );
        Ok(())
    }

    #[test]
    #[ignore]
    fn test_parse_amd_statement() -> Result<(), String> {
        assert_eq!(
            parse_statement("data/example-sold-amd.pdf"),
            Ok((
                vec![],
                vec![],
                vec![
                    (
                        "11/10/23".to_owned(),
                        "11/14/23".to_owned(),
                        72,
                        dec!(118.13),
                        dec!(8505.29),
                        Some("ADVANCED MICRO DEVICES".to_string())
                    ),
                    (
                        "11/22/23".to_owned(),
                        "11/27/23".to_owned(),
                        162,
                        dec!(122.4511),
                        dec!(19836.92),
                        Some("ADVANCED MICRO DEVICES".to_string())
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
