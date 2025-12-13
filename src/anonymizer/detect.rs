// SPDX-FileCopyrightText: 2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

//! PII detection module for anonymizer.
//!
//! This module provides heuristic detection of personally identifiable information (PII)
//! in E*TRADE / Morgan Stanley PDF statement FlateDecode streams. It searches for:
//! - Recipient code (ID)
//! - Name
//! - Address lines (two lines)
//! - Account numbers (spaced and non-spaced formats)
//!
//! Detection is based on anchor text patterns and relative offsets within the token stream.
//! Once all PII categories are found, the module prints a `replace` command suitable for
//! shell invocation with the detected tokens.

use crate::pdf::{extract_texts_from_stream, read_pdf, stream_scanner};
use log::{debug, info, warn};
use std::error::Error;

/// Configuration for locating a token via an anchor text and offset.
///
/// The `text` field identifies an anchor string in the token stream,
/// and `offset` specifies how many tokens ahead the target token is located.
pub(crate) struct AnchorOffset {
    pub text: &'static str,
    pub offset: usize,
}

/// Detection configuration specifying anchor patterns for each PII category.
///
/// Each field is an `AnchorOffset` that defines the anchor text and relative position
/// of the target PII token within the extracted text stream.
pub(crate) struct DetectionConfig {
    pub account: AnchorOffset,
    pub account_spaced: AnchorOffset,
    pub name: AnchorOffset,
    pub recipient_code: AnchorOffset,
    pub recipient_address_line1: AnchorOffset,
    pub recipient_address_line2: AnchorOffset,
}

// Find the first `to_be_redacted = anchor + offset`. Replace all `to_be_redacted` you can find. For most sensitive data.
impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            // [148] 012-345678-910
            account: AnchorOffset {
                text: "Morgan Stanley at Work Self-Directed Account",
                offset: 1,
            },
            // [10] 012 - 345678 - 910 -
            account_spaced: AnchorOffset {
                text: "For the Period",
                offset: 3,
            },
            // [14] JAN KOWALSKI
            name: AnchorOffset {
                text: "FOR:",
                offset: 1,
            },
            /*
            [18] #ABCDEFG
            [19] JAN KOWALSKI
            [20] UL. SWIETOKRZYSKA 12
            [21] WARSAW 00-916 POLAND
            */
            // recipient tokens follow the same anchor; offsets are 1, 3, 4
            recipient_code: AnchorOffset {
                text: "E*TRADE is a business of Morgan Stanley.",
                offset: 1,
            },
            recipient_address_line1: AnchorOffset {
                text: "E*TRADE is a business of Morgan Stanley.",
                offset: 3,
            },
            recipient_address_line2: AnchorOffset {
                text: "E*TRADE is a business of Morgan Stanley.",
                offset: 4,
            },
        }
    }
}

/// Result of PII detection, holding detected tokens for each category.
///
/// Fields are `None` if the corresponding PII was not found.
#[derive(Default, Debug)]
pub(crate) struct DetectionResult {
    id: Option<String>,
    name: Option<String>,
    address_line1: Option<String>,
    address_line2: Option<String>,
    account_spaced: Option<String>,
    account_ms: Option<String>,
}

impl DetectionResult {
    fn all_found(&self) -> bool {
        self.id.is_some()
            && self.name.is_some()
            && self.address_line1.is_some()
            && self.address_line2.is_some()
            && self.account_spaced.is_some()
            && self.account_ms.is_some()
    }
}

/// Detect PII tokens in `input_path` and print a replacement command line.
///
/// The function inspects FlateDecode streams, extracts text tokens and heuristically
/// determines name/address/account tokens. It prints a single `replace` command
/// suitable for shell use.
pub fn detect_pii(input_path: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let pdf_data = read_pdf(input_path)?;

    let config = DetectionConfig::default();
    let mut result = DetectionResult::default();

    for stream in stream_scanner(&pdf_data) {
        if !stream.valid_end_marker {
            warn!(
                "Skipping stream due to end-marker mismatch for object at {}",
                stream.object_start
            );
            continue;
        }
        match extract_texts_from_stream(stream.compressed) {
            Ok(extracted) => {
                result = analyze_extracted_texts(&extracted, &config);
                if result.all_found() {
                    debug!("All target PII categories found.");
                    break;
                }
            }
            Err(e) => {
                warn!(
                    "Failed to extract texts from stream at {}: {}",
                    stream.object_start, e
                );
            }
        }
    }

    let out_path = super::path::anonymous_output_path(input_path);

    // Build final ordered list: name, addr1, addr2, account_spaced, account_ms
    let mut final_texts: Vec<String> = Vec::new();
    let mut inserted = std::collections::HashSet::new();
    if let Some(id) = &result.id {
        if inserted.insert(id.clone()) {
            final_texts.push(id.clone());
        }
    }
    if let Some(n) = &result.name {
        if inserted.insert(n.clone()) {
            final_texts.push(n.clone());
        }
    }
    if let Some(a1) = &result.address_line1 {
        if inserted.insert(a1.clone()) {
            final_texts.push(a1.clone());
        }
    }
    if let Some(a2) = &result.address_line2 {
        if inserted.insert(a2.clone()) {
            final_texts.push(a2.clone());
        }
    }
    if let Some(sp) = &result.account_spaced {
        if inserted.insert(sp.clone()) {
            final_texts.push(sp.clone());
        }
    }
    if let Some(ms) = &result.account_ms {
        if inserted.insert(ms.clone()) {
            final_texts.push(ms.clone());
        }
    }

    print!(
        "replace \"{}\" \"{}\"",
        input_path.display(),
        out_path.display()
    );
    for txt in &final_texts {
        let replacement = "X".repeat(txt.len());
        print!(" \"{}\" \"{}\"", txt, replacement);
    }
    println!();

    Ok(())
}

pub(crate) fn analyze_extracted_texts(
    extracted_texts: &[String],
    config: &DetectionConfig,
) -> DetectionResult {
    debug!("Analyzing {} extracted tokens", extracted_texts.len());
    for (i, txt) in extracted_texts.iter().enumerate() {
        debug!("  [{}] {}", i, txt);
    }

    let id = get_string_by_anchor(&config.recipient_code, extracted_texts);
    let name = get_string_by_anchor(&config.name, extracted_texts);
    let address_line1 = get_string_by_anchor(&config.recipient_address_line1, extracted_texts);
    let address_line2 = get_string_by_anchor(&config.recipient_address_line2, extracted_texts);
    let account_spaced = get_string_by_anchor(&config.account_spaced, extracted_texts);
    let account_ms = get_string_by_anchor(&config.account, extracted_texts);
    // Log what we found or didn't find
    if let Some(ref v) = id {
        info!("Found recipient code: {}", v);
    } else {
        warn!(
            "Recipient code not found via anchor: {}",
            config.recipient_code.text
        );
    }
    if let Some(ref v) = name {
        info!("Found name: {}", v);
    } else {
        warn!("Name not found via anchor: {}", config.name.text);
    }
    if let Some(ref v) = address_line1 {
        info!("Found address_line1: {}", v);
    } else {
        warn!(
            "Address line 1 not found via anchor: {}",
            config.recipient_address_line1.text
        );
    }
    if let Some(ref v) = address_line2 {
        info!("Found address_line2: {}", v);
    } else {
        warn!(
            "Address line 2 not found via anchor: {}",
            config.recipient_address_line2.text
        );
    }
    if let Some(ref v) = account_spaced {
        info!("Found spaced account: {}", v);
    } else {
        warn!(
            "Spaced account not found via anchor: {}",
            config.account_spaced.text
        );
    }
    if let Some(ref v) = account_ms {
        info!("Found ms account: {}", v);
    } else {
        warn!("MS account not found via anchor: {}", config.account.text);
    }

    let acct_validation = validate_account_match(&account_spaced, &account_ms);
    match acct_validation {
        Some(true) => info!("Account validation: MATCH"),
        Some(false) => warn!("Account validation: MISMATCH"),
        None => warn!("Account validation: SKIPPED (missing token)"),
    }
    DetectionResult {
        id,
        name,
        address_line1,
        address_line2,
        account_spaced,
        account_ms,
    }
}

fn get_string_by_anchor(
    anchor_offset: &AnchorOffset,
    extracted_texts: &[String],
) -> Option<String> {
    for (idx, t) in extracted_texts.iter().enumerate() {
        if t.contains(anchor_offset.text) {
            let target_idx = idx + anchor_offset.offset;
            if target_idx < extracted_texts.len() {
                return Some(extracted_texts[target_idx].clone());
            }
        }
    }
    None
}

// Validate account spaced vs non-spaced (compare digits-only). Logs a warning on mismatch.
fn validate_account_match(spaced: &Option<String>, ms: &Option<String>) -> Option<bool> {
    let digits_only = |s: &str| s.chars().filter(|c| c.is_numeric()).collect::<String>();

    match (spaced, ms) {
        (Some(s), Some(m)) => {
            let ds = digits_only(s);
            let dm = digits_only(m);
            Some(ds == dm)
        }
        _ => {
            // One or both values missing; nothing to validate.
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_extracted_texts() {
        // Semi-realistic token stream
        let tokens: Vec<String> = [
            "Beginning Total Value ",
            "$",
            "12,345.67",
            "Ending Total Value ",
            "$1.23",
            "Includes Accrued Interest",
            "CLIENT STATEMENT     ",
            "For the Period September 1",
            "-",
            "30, 2025",
            "012 - 345678 - 910 -",
            "4 - 1",
            "STATEMENT",
            " FOR:",
            "John Doe",
            "",
            "Morgan Stanley Smith Barney LLC. Member SIPC.",
            "E*TRADE is a business of Morgan Stanley.",
            "#ABCDEFG",
            "John Doe",
            "123 Market St",
            "Cityville 12345 WHOKNOWS",
            "Account Details",
            "Morgan Stanley at Work Self-Directed Account",
            "987654321",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let config = DetectionConfig::default();
        let res = analyze_extracted_texts(&tokens, &config);
        assert_eq!(res.name, Some("John Doe".to_string()));
        assert_eq!(res.address_line1, Some("123 Market St".to_string()));
        assert_eq!(
            res.address_line2,
            Some("Cityville 12345 WHOKNOWS".to_string())
        );
        assert_eq!(res.account_ms, Some("987654321".to_string()));
    }

    #[test]
    fn test_validate_account_match_matching() {
        let spaced = Some("012 - 345678 - 910 -".to_string());
        let ms = Some("012345678910".to_string());
        let res = validate_account_match(&spaced, &ms);
        assert_eq!(res, Some(true));
    }

    #[test]
    fn test_validate_account_match_mismatch() {
        let spaced = Some("012 - 345678 - 910 -".to_string());
        let ms = Some("987654321".to_string());
        let res = validate_account_match(&spaced, &ms);
        assert_eq!(res, Some(false));
    }

    #[test]
    fn test_validate_account_match_missing() {
        let spaced: Option<String> = None;
        let ms = Some("987654321".to_string());
        let res = validate_account_match(&spaced, &ms);
        assert_eq!(res, None);
    }
}
