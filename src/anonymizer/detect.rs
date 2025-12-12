use crate::pdf::{extract_texts_from_stream, read_pdf, stream_scanner};
use log::{debug, info, warn};
use std::error::Error;

struct AnchorOffsets {
    text: &'static str,
    offsets: &'static [usize],
}

pub(crate) struct DetectionConfig {
    pub account: AnchorOffsets,
    pub account_spaced: AnchorOffsets,
    pub name: AnchorOffsets,
    pub recipient_data: AnchorOffsets,
}

// Find the first `to_be_redacted = anchor + offset`. Replace all `to_be_redacted` you can find
impl Default for DetectionConfig {
    fn default() -> Self {
        Self {
            // [148] 012-345678-910
            account: AnchorOffsets {
                text: "Morgan Stanley at Work Self-Directed Account",
                offsets: &[1],
            },
            // [10] 012 - 345678 - 910 -
            account_spaced: AnchorOffsets {
                text: "For the Period",
                offsets: &[3],
            },
            // [14] JAN KOWALSKI
            name: AnchorOffsets {
                text: "FOR:",
                offsets: &[1],
            },
            /*
            [18] #BWNJGWM
            [19] JAN KOWALSKI
            [20] UL. SWIETOKRZYSKA 12
            [21] WARSAW 00-916 POLAND
            */
            recipient_data: AnchorOffsets {
                text: "E*TRADE is a business of Morgan Stanley.",
                offsets: &[1, 2, 3, 4],
            },
        }
    }
}

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

    let mut result = DetectionResult::default();
    let config = DetectionConfig::default();

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
                analyze_extracted_texts(&extracted, &mut result, &config);
                if result.all_found() {
                    debug!("All target PII categories found; stopping search early.");
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
    result: &mut DetectionResult,
    config: &DetectionConfig,
) {
    debug!("Analyzing {} extracted tokens", extracted_texts.len());
    for (i, txt) in extracted_texts.iter().enumerate() {
        debug!("  [{}] {}", i, txt);
    }
    // Run the composed helpers (implemented as top-level private helpers)
    if find_account_after_anchor_in_stream(extracted_texts, result, config) {
        return;
    }
    let for_search_start = find_spaced_account_and_start(extracted_texts, result, config);
    handle_for_and_extract(extracted_texts, for_search_start, result, config);
    validate_account_match(result);
}

// helper: if address lines already known, look for the anchor in this stream and pick following token
fn find_account_after_anchor_in_stream(
    extracted_texts: &[String],
    result: &mut DetectionResult,
    config: &DetectionConfig,
) -> bool {
    if result.address_line1.is_some()
        && result.address_line2.is_some()
        && result.account_ms.is_none()
    {
        let anchor_text = config.account.text;
        for (idx, t) in extracted_texts.iter().enumerate() {
            if t.contains(anchor_text) {
                let mut next = idx + 1;
                while next < extracted_texts.len() {
                    let cand_full = &extracted_texts[next];
                    if !cand_full.is_empty() {
                        info!(
                            "Found account number after anchor (later stream): {}",
                            cand_full
                        );
                        result.account_ms = Some(cand_full.clone());
                        return true;
                    }
                    next += 1;
                }
            }
        }
    }
    false
}

// look for spaced account after "For the Period" and return start index for FOR: scanning
fn find_spaced_account_and_start(
    extracted_texts: &[String],
    result: &mut DetectionResult,
    config: &DetectionConfig,
) -> usize {
    let mut for_search_start: usize = 0;
    for (i, txt) in extracted_texts.iter().enumerate() {
        if txt.contains(config.account_spaced.text) && i + 3 < extracted_texts.len() {
            let account_full = extracted_texts[i + 3].clone();
            let account = account_full.as_str();
            if account.contains(" - ") && account.chars().any(|c| c.is_numeric()) {
                info!(
                    "Found account number (with spaces) after 'For the Period': {}",
                    account
                );
                if result.account_spaced.is_none() {
                    result.account_spaced = Some(account_full.clone());
                }
                // start FOR: search after the account token
                for_search_start = i + 4; // i+3 is account token, so start after
                break;
            }
        }
    }
    for_search_start
}

// handle FOR: marker - extract name and next two non-empty tokens as address lines; attempt anchor-based ms account after
fn handle_for_and_extract(
    extracted_texts: &[String],
    start: usize,
    result: &mut DetectionResult,
    config: &DetectionConfig,
) {
    for (i, txt) in extracted_texts.iter().enumerate().skip(start) {
        if txt.contains(config.name.text) && i + 1 < extracted_texts.len() {
            let name_full = extracted_texts[i + 1].clone();
            let name = name_full.as_str();
            if !name.is_empty() {
                let mut ctx: Vec<String> = Vec::new();
                for j in 0..4 {
                    if i + 1 + j < extracted_texts.len() {
                        ctx.push(extracted_texts[i + 1 + j].clone());
                    }
                }
                info!(
                    "Found name after 'FOR:': {} -- context: {:?}",
                    name_full, ctx
                );
                if result.name.is_none() {
                    result.name = Some(name_full.clone());
                }
            }

            // Deterministic rule: unconditionally capture the next two non-empty tokens after the name.
            // Prefer a later occurrence of the same name (some PDFs repeat the name and the address appears after the second occurrence).
            let mut anchor_index = i + 1; // default: position of the name after FOR:
            for k in (i + 2)..extracted_texts.len() {
                if extracted_texts[k].contains(&name_full) {
                    anchor_index = k;
                    break;
                }
            }

            // If we found a later occurrence, check for ID immediately before it.
            if anchor_index > i + 1 {
                let id_candidate = &extracted_texts[anchor_index - 1];
                if !id_candidate.is_empty() {
                    info!("Found ID before name anchor: {}", id_candidate);
                    result.id = Some(id_candidate.clone());
                }
            }

            let mut collected = 0;
            let mut look = 1; // start looking after the anchor name
            while collected < 2 && anchor_index + look < extracted_texts.len() {
                let candidate_full = extracted_texts[anchor_index + look].clone();
                let candidate = candidate_full.as_str();
                look += 1;
                if candidate.is_empty() {
                    continue;
                }

                // Always capture the next two non-empty tokens as address lines.
                collected += 1;
                if collected == 1 {
                    info!(
                        "Captured address_line1 after name (anchor_index={}): {} -- token_index={}",
                        anchor_index,
                        candidate,
                        anchor_index + look - 1
                    );
                    if result.address_line1.is_none() {
                        result.address_line1 = Some(candidate_full.clone());
                    }
                } else {
                    info!(
                        "Captured address_line2 after name (anchor_index={}): {} -- token_index={}",
                        anchor_index,
                        candidate,
                        anchor_index + look - 1
                    );
                    if result.address_line2.is_none() {
                        result.address_line2 = Some(candidate_full.clone());
                    }
                }
            }

            // Immediately after capturing the two address lines, pick the first non-empty token
            // that follows anchor
            if result.address_line1.is_some() && result.address_line2.is_some() {
                // First: look for the specific preceding anchor and take the next token.
                let mut found_via_anchor = false;
                let anchor_text = config.account.text;
                let mut anchor_idx = None;
                for idx in (anchor_index + look)..extracted_texts.len() {
                    if extracted_texts[idx].contains(anchor_text) {
                        anchor_idx = Some(idx);
                        break;
                    }
                }

                if let Some(ai) = anchor_idx {
                    let mut next = ai + 1;
                    while next < extracted_texts.len() {
                        let cand_full = extracted_texts[next].clone();
                        if !cand_full.is_empty() {
                            info!(
                                "Found account number after anchor '{}' : {}",
                                anchor_text, cand_full
                            );
                            result.account_ms = Some(cand_full.clone());
                            found_via_anchor = true;
                            break;
                        }
                        next += 1;
                    }
                }

                if found_via_anchor {
                    return; // found via anchor, we're done
                }
            }
        }
    }
}

// Validate account spaced vs non-spaced (compare digits-only)
fn validate_account_match(result: &DetectionResult) {
    if let (Some(spaced), Some(ms)) = (&result.account_spaced, &result.account_ms) {
        let digits_only = |s: &str| s.chars().filter(|c| c.is_numeric()).collect::<String>();
        let ds = digits_only(spaced);
        let dm = digits_only(ms);
        if ds == dm {
            info!(
                "Validated account: spaced='{}' matches non-spaced='{}'",
                spaced, ms
            );
        } else {
            warn!(
                "Account mismatch: spaced='{}' vs non-spaced='{}' (digits: {} != {})",
                spaced, ms, ds, dm
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_spaced_account_after_for_period() {
        // Simulate a small token stream that might appear near the account header
        let tokens = vec![
            "Account Summary".to_string(),
            "For the Period September 1".to_string(),
            "-".to_string(),
            "30, 2025".to_string(),
            "123 - 456789 - 012".to_string(),
        ];
        let mut res = DetectionResult::default();
        let config = DetectionConfig::default();
        analyze_extracted_texts(&tokens, &mut res, &config);
        assert_eq!(res.account_spaced, Some("123 - 456789 - 012".to_string()));
    }

    #[test]
    fn test_for_name_and_address_extraction_and_anchor_account() {
        // Realistic token stream: FOR: name, address tokens, then account anchor and number
        let tokens = vec![
            "FOR:".to_string(),
            "John Doe".to_string(),
            "123 Market St".to_string(),
            "Cityville 12345".to_string(),
            "Account Details".to_string(),
            "Morgan Stanley at Work Self-Directed Account".to_string(),
            "987654321".to_string(),
        ];
        let mut res = DetectionResult::default();
        let config = DetectionConfig::default();
        analyze_extracted_texts(&tokens, &mut res, &config);
        assert_eq!(res.name, Some("John Doe".to_string()));
        assert_eq!(res.address_line1, Some("123 Market St".to_string()));
        assert_eq!(res.address_line2, Some("Cityville 12345".to_string()));
        assert_eq!(res.account_ms, Some("987654321".to_string()));
    }
}
