// SPDX-FileCopyrightText: 2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

//! PDF parsing utilities: header validation, stream extraction, text token parsing.
//! This module is intentionally strict and only supports a narrow subset of PDF
//! objects used by the target documents: FlateDecode streams with explicit /Length.

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use log::{debug, error, info, warn};
use regex::bytes::Regex;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};

// Centralized constants and helpers for PDF parsing to reduce duplication between detect/replace.
/// Expected PDF header (strictly enforced).
pub(crate) const PDF_HEADER: &[u8] = b"%PDF-1.3\n";
/// Regex matching an object with FlateDecode stream and explicit /Length.
pub(crate) const OBJ_STREAM_RE: &str = r"(?s)\d+\s+\d+\s+obj\s*<<\s*/Length\s+(\d+)\s*/Filter\s*\[\s*/FlateDecode\s*\]\s*>>\s*stream\n";

/// Read entire PDF file and validate strict header.
pub(crate) fn read_pdf(path: &std::path::Path) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut file = File::open(path)?;
    let mut pdf_data = Vec::new();
    file.read_to_end(&mut pdf_data)?;
    if pdf_data.len() < PDF_HEADER.len() || &pdf_data[0..PDF_HEADER.len()] != PDF_HEADER {
        error!(
            "Unsupported PDF version or invalid PDF header at '{}'.",
            path.display()
        );
        return Err("Invalid PDF header".into());
    }
    Ok(pdf_data)
}

// Lightweight representation of a PDF flate stream for detection-only workflow.
/// Lightweight representation of a FlateDecode stream slice inside a PDF.
pub(crate) struct StreamData<'a> {
    pub object_start: usize,
    pub data_start: usize,
    pub compressed: &'a [u8],
    pub valid_end_marker: bool,
}

/// Iterator over stream objects, avoiding allocating a full Vec upfront.
pub(crate) struct StreamScanner<'a> {
    re: Regex,
    data: &'a [u8],
    search_from: usize,
}

/// Create a new streaming iterator over PDF FlateDecode objects.
pub(crate) fn stream_scanner<'a>(pdf_data: &'a [u8]) -> StreamScanner<'a> {
    StreamScanner {
        re: Regex::new(OBJ_STREAM_RE).unwrap(),
        data: pdf_data,
        search_from: 0,
    }
}

impl<'a> Iterator for StreamScanner<'a> {
    type Item = StreamData<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.search_from < self.data.len() {
            // Use captures_at to find next match at current position
            if let Some(caps) = self.re.captures_at(self.data, self.search_from) {
                let whole = caps.get(0)?;
                // advance search past this object to avoid infinite loop
                self.search_from = whole.end();
                if let Some((compressed, data_start, valid)) =
                    extract_stream_bytes(self.data, &caps)
                {
                    return Some(StreamData {
                        object_start: whole.start(),
                        data_start,
                        compressed,
                        valid_end_marker: valid,
                    });
                } else {
                    continue; // skip invalid capture
                }
            } else {
                self.search_from = self.data.len();
            }
        }
        None
    }
}

/// Given a regex capture for an object, validate endmarker and return compressed stream bytes
/// Given a capture for a stream object, validate end marker and return the raw compressed data plus a validity flag.
pub(crate) fn extract_stream_bytes<'a>(
    pdf_data: &'a [u8],
    caps: &regex::bytes::Captures<'a>,
) -> Option<(&'a [u8], usize, bool)> {
    // Strict project rule: expected end marker is fixed here
    const EXPECTED_END: &[u8] = b"\nendstream\nendobj";
    // Validate capture groups
    let whole = match caps.get(0) {
        Some(m) => m,
        None => {
            error!("PDF object capture missing whole-match");
            return None;
        }
    };
    let length_bytes = match caps.get(1) {
        Some(m) => m.as_bytes(),
        None => {
            error!(
                "PDF object capture missing /Length group at object starting {}",
                whole.start()
            );
            return None;
        }
    };

    // Parse length strictly; if it fails, we consider this object invalid
    let length = match std::str::from_utf8(length_bytes)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
    {
        Some(v) => v,
        None => {
            error!(
                "Invalid /Length value '{}' in object starting at {}",
                String::from_utf8_lossy(length_bytes),
                whole.start()
            );
            return None;
        }
    };

    let data_start = whole.end();
    let stream_end = match data_start.checked_add(length) {
        Some(v) => v,
        None => {
            error!(
                "Stream end overflow for object at {} (length={})",
                data_start, length
            );
            return None;
        }
    };

    // strict bounds checks: must be entirely within pdf_data
    if stream_end > pdf_data.len() {
        error!(
            "Stream end out of bounds for object starting at {}: stream_end={} pdf_len={}",
            data_start,
            stream_end,
            pdf_data.len()
        );
        return None;
    }
    if stream_end + EXPECTED_END.len() > pdf_data.len() {
        error!(
            "End marker out of bounds after stream_end {} for object starting at {} (pdf_len={})",
            stream_end,
            data_start,
            pdf_data.len()
        );
        return None;
    }

    // Validate exact end marker (requirements are strict)
    let debug_slice = &pdf_data[stream_end..stream_end + EXPECTED_END.len()];
    if debug_slice != EXPECTED_END {
        warn!(
            "End marker mismatch for object starting at {}: found {:?}, expected {:?}",
            data_start, debug_slice, EXPECTED_END
        );
        // Return decompressed candidate but indicate end marker mismatch for caller decision
        return Some((&pdf_data[data_start..stream_end], data_start, false));
    }

    Some((&pdf_data[data_start..stream_end], data_start, true))
}

/// Decompress stream and extract text tokens from PDF text operators
/// Decompress a FlateDecode stream and extract text tokens appearing in `( .. ) Tj` operators.
/// Handles escaped parentheses `\(` and `\)` within PDF string literals.
pub(crate) fn extract_texts_from_stream(
    compressed_data: &[u8],
) -> Result<Vec<String>, Box<dyn Error>> {
    let mut decoder = ZlibDecoder::new(compressed_data);
    let mut decompressed = Vec::new();
    decoder.read_to_end(&mut decompressed)?;
    // Updated regex to handle escaped characters: (?:[^)\\]|\\.)* matches either
    // a non-special char OR a backslash followed by any char (handles \(, \), \\, \n, etc.)
    let text_re =
        Regex::new(r"\(((?:[^)\\]|\\.)*)\)\s*Tj").map_err(|e| Box::new(e) as Box<dyn Error>)?;
    let mut extracted_texts: Vec<String> = Vec::new();
    for text_caps in text_re.captures_iter(&decompressed) {
        if let Some(txt) = text_caps.get(1) {
            let text_bytes = txt.as_bytes();
            // Unescape PDF string literal escape sequences
            let unescaped = unescape_pdf_string(text_bytes);
            extracted_texts.push(String::from_utf8_lossy(&unescaped).to_string());
        }
    }

    Ok(extracted_texts)
}

/// Unescape PDF string literal escape sequences per PDF 1.3 spec (Table 3.2).
/// Handles: \n \r \t \b \f \( \) \\ and \ddd (octal, 1-3 digits).
/// Per spec: "If the character following the backslash is not one of those shown
/// in the table, the backslash is ignored."
fn unescape_pdf_string(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(data.len());
    let mut i = 0;

    while i < data.len() {
        if data[i] == b'\\' && i + 1 < data.len() {
            let (output, bytes_consumed) = handle_pdf_escape(&data[i + 1..]);
            if let Some(byte) = output {
                result.push(byte);
            }
            i += bytes_consumed;
        } else {
            result.push(data[i]);
            i += 1;
        }
    }
    result
}

/// Handle a single PDF escape sequence starting after the backslash.
/// Returns (output byte if any, number of bytes to advance including the backslash).
fn handle_pdf_escape(data: &[u8]) -> (Option<u8>, usize) {
    if data.is_empty() {
        return (None, 1); // Lone backslash at end
    }

    match data[0] {
        b'n' => (Some(b'\n'), 2),
        b'r' => (Some(b'\r'), 2),
        b't' => (Some(b'\t'), 2),
        b'b' => (Some(b'\x08'), 2), // backspace
        b'f' => (Some(b'\x0C'), 2), // form feed
        b'(' => (Some(b'('), 2),
        b')' => (Some(b')'), 2),
        b'\\' => (Some(b'\\'), 2),
        b'0'..=b'7' => parse_pdf_octal_escape(data),
        // Per spec: ignore backslash for unrecognized escapes
        _ => (Some(data[0]), 2),
    }
}

/// Parse octal escape sequence \ddd (1-3 octal digits).
/// Returns (parsed byte, bytes consumed including backslash).
fn parse_pdf_octal_escape(data: &[u8]) -> (Option<u8>, usize) {
    let mut end = 0;
    // Consume up to 3 octal digits
    while end < data.len() && end < 3 && data[end].is_ascii_digit() && data[end] <= b'7' {
        end += 1;
    }

    if let Ok(octal_str) = std::str::from_utf8(&data[..end]) {
        if let Ok(value) = u8::from_str_radix(octal_str, 8) {
            return (Some(value), end + 1); // +1 for the backslash
        }
    }

    // Fallback: ignore backslash if parsing fails
    (Some(data[0]), 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unescape_simple_escapes() {
        // Test all simple escape sequences
        assert_eq!(unescape_pdf_string(br"\n"), b"\n");
        assert_eq!(unescape_pdf_string(br"\r"), b"\r");
        assert_eq!(unescape_pdf_string(br"\t"), b"\t");
        assert_eq!(unescape_pdf_string(br"\b"), b"\x08"); // backspace
        assert_eq!(unescape_pdf_string(br"\f"), b"\x0C"); // form feed
        assert_eq!(unescape_pdf_string(br"\("), b"(");
        assert_eq!(unescape_pdf_string(br"\)"), b")");
        assert_eq!(unescape_pdf_string(br"\\"), b"\\");
    }

    #[test]
    fn test_unescape_octal_sequences() {
        // Single digit octal
        assert_eq!(unescape_pdf_string(br"\0"), b"\x00");
        assert_eq!(unescape_pdf_string(br"\7"), b"\x07");

        // Two digit octal
        assert_eq!(unescape_pdf_string(br"\53"), b"+"); // \053 = 43 decimal = '+'

        // Three digit octal
        assert_eq!(unescape_pdf_string(br"\053"), b"+");
        assert_eq!(unescape_pdf_string(br"\245"), b"\xA5"); // 165 decimal
        assert_eq!(unescape_pdf_string(br"\307"), b"\xC7"); // 199 decimal

        // Octal followed by non-digit (from PDF spec example)
        assert_eq!(unescape_pdf_string(br"\0053"), b"\x053"); // \005 + '3'
    }

    #[test]
    fn test_unescape_real_world_case() {
        // The actual case from the PDF that was failing
        assert_eq!(
            unescape_pdf_string(br"NET CREDITS/\(DEBITS\)"),
            b"NET CREDITS/(DEBITS)"
        );

        // Dollar amount with parentheses
        assert_eq!(unescape_pdf_string(br"\(6,085.80\)"), b"(6,085.80)");

        // Date range
        assert_eq!(
            unescape_pdf_string(br"\(9/1/25-9/30/25\)"),
            b"(9/1/25-9/30/25)"
        );
    }

    #[test]
    fn test_unescape_unrecognized_escape() {
        // Per spec: "If the character following the backslash is not one of those
        // shown in the table, the backslash is ignored."
        assert_eq!(unescape_pdf_string(br"\x"), b"x");
        assert_eq!(unescape_pdf_string(br"\q"), b"q");
        assert_eq!(unescape_pdf_string(br"\Z"), b"Z");
    }

    #[test]
    fn test_unescape_mixed_content() {
        // Mix of regular text, escapes, and parentheses
        assert_eq!(
            unescape_pdf_string(br"Hello\nWorld\t\(test\)"),
            b"Hello\nWorld\t(test)"
        );

        // \\ becomes \, then 053 is literal text (not preceded by backslash after unescape)
        // Then \245 becomes byte 0xA5
        assert_eq!(
            unescape_pdf_string(br"Price: \(\\053\245\)"),
            b"Price: (\\053\xA5)"
        );
    }

    #[test]
    fn test_unescape_edge_cases() {
        // Empty string
        assert_eq!(unescape_pdf_string(b""), b"");

        // No escapes
        assert_eq!(unescape_pdf_string(b"plain text"), b"plain text");

        // Backslash at end (no following character)
        assert_eq!(unescape_pdf_string(b"text\\"), b"text\\");
    }
}

// === Stream replacement & recompression utilities (migrated from streams.rs) ===

/// Replace all non-overlapping occurrences of `search` with `replace` in `data`.
fn replace_bytes_all_occurrences(data: &[u8], search: &[u8], replace: &[u8]) -> (Vec<u8>, usize) {
    let mut result = Vec::new();
    let mut pos = 0;
    let mut count = 0;
    while pos < data.len() {
        if pos + search.len() <= data.len() && &data[pos..pos + search.len()] == search {
            result.extend_from_slice(replace);
            pos += search.len();
            count += 1;
        } else {
            result.push(data[pos]);
            pos += 1;
        }
    }
    (result, count)
}

/// Try progressive zlib compression levels (0..=9) returning the first compressed form whose length is <= `max_size`.
fn find_fitting_compression(data: &[u8], max_size: usize) -> Option<(Vec<u8>, u32)> {
    for level in 0..=9 {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(level));
        if encoder.write_all(data).is_err() {
            continue;
        }
        let compressed = encoder.finish().ok()?;
        if compressed.len() <= max_size {
            return Some((compressed, level));
        }
    }
    None
}

/// Decompress a stream, apply all replacements, and recompress if possible within
/// the original compressed size. Returns new compressed bytes and per-pattern counts.
pub(crate) fn process_stream(
    compressed_data: &[u8],
    replacements: &[(String, String)],
) -> Result<(Vec<u8>, std::collections::HashMap<String, usize>), Box<dyn Error>> {
    let original_len = compressed_data.len();
    let mut decoder = ZlibDecoder::new(compressed_data);
    let mut decompressed = Vec::new();
    match decoder.read_to_end(&mut decompressed) {
        Ok(_) => {
            debug!("Decompressed: {} B", decompressed.len());
            let mut modified_data = decompressed.clone();
            let mut found_any = false;
            let mut per_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for (needle, repl) in replacements {
                let (new_data, occurrences) = replace_bytes_all_occurrences(
                    &modified_data,
                    needle.as_bytes(),
                    repl.as_bytes(),
                );
                if occurrences > 0 {
                    debug!("Found '{}' {} times", needle, occurrences);
                    modified_data = new_data;
                    per_counts.insert(needle.clone(), occurrences);
                    found_any = true;
                }
            }
            if found_any {
                if let Some((fitting, level)) =
                    find_fitting_compression(&modified_data, original_len)
                {
                    debug!(
                        "Compression level {} produced {} B (<= {} B)",
                        level,
                        fitting.len(),
                        original_len
                    );
                    info!(
                        "Compressed stream with level {} ({} B)",
                        level,
                        fitting.len()
                    );
                    return Ok((fitting, per_counts));
                } else {
                    warn!(
                        "All compression levels exceed original size {}; keeping original. PII MAY REMAIN EXPOSED!",
                        original_len
                    );
                    info!(
                        "Falling back to original compressed stream ({} B)",
                        original_len
                    );
                }
            }
        }
        Err(e) => {
            error!("Decompression error: {}", e);
        }
    }
    Ok((compressed_data.to_vec(), std::collections::HashMap::new()))
}
