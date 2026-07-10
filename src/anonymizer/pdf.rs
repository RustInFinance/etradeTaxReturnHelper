// SPDX-FileCopyrightText: 2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

//! PDF parsing utilities: header validation, stream extraction, text token parsing.
//! This module is intentionally strict and only supports a narrow subset of PDF
//! objects used by the target documents: FlateDecode streams with explicit /Length.

use flate2::read::ZlibDecoder;
use log::{error, warn};
use regex::bytes::Regex;
use std::error::Error;
use std::fs::File;
use std::io::Read;

// Centralized constants and helpers for PDF parsing to reduce duplication between list/replace.
/// Expected PDF header (strictly enforced).
pub(crate) const PDF_HEADER: &[u8] = b"%PDF-1.3";
/// Regex matching any stream object with an explicit `/Length`.
/// Uses (?s) DOTALL so the dictionary may span newlines; `[^>]` keeps a match
/// within a single dictionary (no nested `>`), matching the narrow object shapes
/// these documents use. The scanner then classifies each match by inspecting the
/// dictionary text only (never the binary payload), in this order:
/// - contains `/Length1` -> embedded font program (FontFile/FontFile2), skipped
///   regardless of compression: binary font data has no PDF text tokens and must
///   never be modified,
/// - otherwise contains `/FlateDecode` -> compressed (zlib-decoded and scanned),
/// - otherwise contains `/Filter` -> unsupported filter (e.g. `/DCTDecode`
///   image), skipped,
/// - otherwise -> genuine uncompressed content stream, scanned verbatim.
pub(crate) const OBJ_STREAM_RE: &str =
    r"(?s)\d+\s+\d+\s+obj\s*<<[^>]*?/Length\s+(\d+)[^>]*?>>\s*stream\r?\n";

/// Read entire PDF file and validate strict header.
pub fn read_pdf(path: &std::path::Path) -> Result<Vec<u8>, Box<dyn Error>> {
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

/// Lightweight representation of a stream slice inside a PDF (compressed or uncompressed).
pub struct StreamData<'a> {
    pub object_start: usize,
    pub data_start: usize,
    pub compressed: &'a [u8],
    pub valid_end_marker: bool,
    pub is_compressed: bool,
}

/// Iterator over stream objects, avoiding allocating a full Vec upfront.
pub struct StreamScanner<'a> {
    re: Regex,
    data: &'a [u8],
    search_from: usize,
}

/// Create a new streaming iterator over PDF stream objects (compressed and uncompressed).
pub fn stream_scanner<'a>(pdf_data: &'a [u8]) -> StreamScanner<'a> {
    StreamScanner {
        re: Regex::new(OBJ_STREAM_RE).unwrap(),
        data: pdf_data,
        search_from: 0,
    }
}

/// True if the byte slice `haystack` contains the contiguous subslice `needle`.
fn bytes_contain(haystack: &[u8], needle: &[u8]) -> bool {
    needle.len() <= haystack.len() && haystack.windows(needle.len()).any(|w| w == needle)
}

impl<'a> Iterator for StreamScanner<'a> {
    type Item = StreamData<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        while self.search_from < self.data.len() {
            let caps = match self.re.captures_at(self.data, self.search_from) {
                Some(c) => c,
                None => {
                    self.search_from = self.data.len();
                    return None;
                }
            };

            let whole = caps.get(0)?;
            self.search_from = whole.end();

            // Classify by inspecting only the matched object header/dictionary
            // (never the binary payload, which begins after `stream`).
            let dict = whole.as_bytes();
            let is_compressed = if bytes_contain(dict, b"/Length1") {
                // Embedded font program (FontFile/FontFile2): binary font data
                // with no PDF text tokens. Checked first so that even a
                // Flate-compressed font is skipped, never scanned/modified.
                continue;
            } else if bytes_contain(dict, b"/FlateDecode") {
                true
            } else if bytes_contain(dict, b"/Filter") {
                // Unsupported filter (e.g. /DCTDecode image): don't decode/scan.
                continue;
            } else {
                false
            };

            if let Some((data, data_start, valid)) = extract_stream_bytes(self.data, &caps) {
                return Some(StreamData {
                    object_start: whole.start(),
                    data_start,
                    compressed: data,
                    valid_end_marker: valid,
                    is_compressed,
                });
            } else {
                continue; // skip invalid capture
            }
        }
        None
    }
}

/// Given a capture for a stream object, validate the end marker and return the raw stream bytes plus a validity flag.
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

/// Extracted text token: the decoded string plus its start/end byte offsets
/// within the decompressed stream.
pub type TextToken = (String, usize, usize);

/// Extract texts with positions from a compressed or uncompressed stream.
pub fn extract_texts_from_stream(
    stream_data: &[u8],
    is_compressed: bool,
) -> Result<Vec<TextToken>, Box<dyn Error>> {
    if is_compressed {
        let mut decoder = ZlibDecoder::new(stream_data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)?;
        Ok(scan_decompressed_text(&decompressed))
    } else {
        Ok(scan_decompressed_text(stream_data))
    }
}

/// Extract text tokens (with positions) from an already-decompressed content stream.
///
/// Use this when the caller has already decompressed the stream to avoid a second
/// decompression pass.
pub fn extract_texts_from_decompressed(decompressed: &[u8]) -> Vec<TextToken> {
    scan_decompressed_text(decompressed)
}

/// association of text literal bytes and their positions in the decompressed buffer.
fn scan_decompressed_text(decompressed: &[u8]) -> Vec<(String, usize, usize)> {
    // Linear scanner to find parenthesized literal strings reliably
    // and associate them with text operators. This avoids brittle regexes
    // and correctly handles escapes, octal sequences and nested parentheses.
    fn skip_whitespace(buf: &[u8], mut idx: usize) -> usize {
        while idx < buf.len() {
            match buf[idx] {
                b' ' | b'\t' | b'\n' | b'\r' | 0x0C => idx += 1,
                _ => break,
            }
        }
        idx
    }

    fn parse_literal(buf: &[u8], open_idx: usize) -> Option<(usize, usize)> {
        // open_idx points to '('
        let mut i = open_idx + 1;
        let mut depth: i32 = 1;
        while i < buf.len() {
            match buf[i] {
                b'\\' => {
                    // escape: skip next byte if present (octal handled by unescape)
                    i += 1;
                    if i < buf.len() {
                        // skip the escaped character
                        i += 1;
                    }
                }
                b'(' => {
                    depth += 1;
                    i += 1;
                }
                b')' => {
                    depth -= 1;
                    i += 1;
                    if depth == 0 {
                        // return indexes of inner content (exclude parentheses)
                        return Some((open_idx + 1, i - 1));
                    }
                }
                _ => i += 1,
            }
        }
        None
    }

    let mut extracted: Vec<(String, usize, usize)> = Vec::new();
    // Tokens found since the most recent `BT`. They are committed to `extracted`
    // only when a matching `ET` closes the text object. A stray `BT` that never
    // closes (typical when non-content binary slips through) leaves its pending
    // tokens uncommitted, so we never rewrite coincidental byte patterns.
    let mut pending: Vec<(String, usize, usize)> = Vec::new();
    let mut i = 0usize;
    let mut in_text_object = false;

    while i < decompressed.len() {
        // Look for BT/ET operators to track text object state
        if i + 1 < decompressed.len() {
            if &decompressed[i..i + 2] == b"BT" {
                in_text_object = true;
                i += 2;
                continue;
            } else if &decompressed[i..i + 2] == b"ET" {
                in_text_object = false;
                // Commit only fully-closed text objects.
                extracted.append(&mut pending);
                i += 2;
                continue;
            }
        }

        match decompressed[i] {
            b'(' if in_text_object => {
                if let Some((s, e)) = parse_literal(decompressed, i) {
                    // after the closing paren at index e+1, check for text operators
                    let after = skip_whitespace(decompressed, e + 1);
                    let mut is_text = false;
                    if after < decompressed.len() {
                        let op1 = decompressed[after];
                        if op1 == b'\'' || op1 == b'"' {
                            is_text = true;
                        } else if after + 1 < decompressed.len() {
                            let op2 = &decompressed[after..after + 2];
                            if op2 == b"Tj" || op2 == b"TJ" {
                                is_text = true;
                            }
                        }
                    }

                    if is_text {
                        let raw = &decompressed[s..e];
                        let unescaped = unescape_pdf_string(raw);
                        pending.push((String::from_utf8_lossy(&unescaped).to_string(), s, e));
                    }
                    i = e + 1; // continue after closing paren
                } else {
                    break; // malformed string — stop scanning
                }
            }
            b'[' if in_text_object => {
                // parse array until matching ']', collecting inner literal items
                let mut arr_i = i + 1;
                let mut found_end = false;
                let mut inner_literals: Vec<(usize, usize)> = Vec::new();
                while arr_i < decompressed.len() {
                    match decompressed[arr_i] {
                        b'(' => {
                            if let Some((s, e)) = parse_literal(decompressed, arr_i) {
                                inner_literals.push((s, e));
                                arr_i = e + 1;
                            } else {
                                break; // malformed
                            }
                        }
                        b']' => {
                            // check for TJ operator after the array
                            let after = skip_whitespace(decompressed, arr_i + 1);
                            if after + 1 < decompressed.len()
                                && &decompressed[after..after + 2] == b"TJ"
                            {
                                for (s, e) in inner_literals.iter() {
                                    let raw = &decompressed[*s..*e];
                                    let unescaped = unescape_pdf_string(raw);
                                    pending.push((
                                        String::from_utf8_lossy(&unescaped).to_string(),
                                        *s,
                                        *e,
                                    ));
                                }
                            }
                            i = arr_i;
                            found_end = true;
                            break;
                        }
                        b'\\' => {
                            // skip escaped char inside array (not inside literal)
                            arr_i += 1;
                            if arr_i < decompressed.len() {
                                arr_i += 1;
                            }
                        }
                        _ => arr_i += 1,
                    }
                }
                if !found_end {
                    break; // malformed array — stop scanning to avoid infinite loop
                }
            }
            _ => i += 1,
        }
    }

    // sort and dedup by start index
    extracted.sort_by_key(|t| t.1);
    extracted.dedup_by(|a, b| a.1 == b.1);
    extracted
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

    #[test]
    fn test_scan_array_operator_boundaries() {
        // Simulation: [ (text) ] TJ
        // Ensures the parser stops exactly at ']' and doesn't skip the next byte.
        let buf = b"BT [(text)] TJ ET";
        let extracted = scan_decompressed_text(buf);

        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].0, "text");
    }

    #[test]
    fn test_nested_parentheses() {
        // PDF spec allows nested parens if balanced: (Text (inner) more)
        let buf = b"BT (Outer (inner) text) Tj ET";
        let extracted = scan_decompressed_text(buf);
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].0, "Outer (inner) text");
    }

    #[test]
    fn test_binary_junk_resilience() {
        // Stream containing binary junk that might look like operators
        let mut buf = Vec::new();
        buf.extend_from_slice(b"BT (Valid) Tj ");
        buf.push(0x01);
        buf.push(0x28); // binary '('
        buf.extend_from_slice(b"JUNK");
        buf.push(0x29); // binary ')'
        buf.extend_from_slice(b" ET");

        let extracted = scan_decompressed_text(&buf);
        // Should find "Valid" and the junk string, but not crash
        assert!(extracted.iter().any(|(s, _, _)| s == "Valid"));
        assert!(!extracted.is_empty());
    }

    #[test]
    fn test_array_operator_at_et_boundary() {
        // Test: [(text)]TJ ET
        // This checks if the parser correctly handles the end of the text block immediately after an array.
        let buf = b"BT [(text)]TJ ET";
        let extracted = scan_decompressed_text(buf);
        assert_eq!(extracted.len(), 1);
        assert_eq!(extracted[0].0, "text");
    }

    #[test]
    fn test_empty_and_max_length_literals() {
        // Test edge cases for literal lengths
        let buf = b"BT () Tj (A) Tj (BC) Tj ET";
        let extracted = scan_decompressed_text(buf);
        assert_eq!(extracted.len(), 3);
        assert_eq!(extracted[0].0, "");
        assert_eq!(extracted[1].0, "A");
        assert_eq!(extracted[2].0, "BC");
    }

    // --- Synthetic stream_scanner classification tests ---
    // These build tiny in-memory PDF snippets (no real fixtures) to exercise how
    // stream_scanner distinguishes FlateDecode, plain uncompressed, and non-Flate
    // filtered (e.g. /DCTDecode image) streams.

    /// Zlib-compress `data` the same way real FlateDecode streams are stored.
    fn flate_compress(data: &[u8]) -> Vec<u8> {
        use flate2::write::ZlibEncoder;
        use flate2::Compression;
        use std::io::Write;
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data).unwrap();
        enc.finish().unwrap()
    }

    /// Wrap raw stream bytes in a minimal `N 0 obj << /Length L{extra} >> stream ... endstream endobj`.
    /// `extra` holds any additional dictionary entries (e.g. ` /Filter [/FlateDecode]`).
    fn build_stream_object(obj_num: u32, extra_dict: &str, body: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(
            format!(
                "{} 0 obj\n<< /Length {}{} >>\nstream\n",
                obj_num,
                body.len(),
                extra_dict
            )
            .as_bytes(),
        );
        buf.extend_from_slice(body);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        buf
    }

    #[test]
    fn test_scanner_classifies_flate_stream_as_compressed() {
        let compressed = flate_compress(b"BT (SECRET) Tj ET");
        let pdf = build_stream_object(1, " /Filter [/FlateDecode]", &compressed);

        let streams: Vec<_> = stream_scanner(&pdf).collect();
        assert_eq!(streams.len(), 1);
        assert!(streams[0].is_compressed);
        assert!(streams[0].valid_end_marker);

        let texts =
            extract_texts_from_stream(streams[0].compressed, streams[0].is_compressed).unwrap();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].0, "SECRET");
    }

    #[test]
    fn test_scanner_classifies_plain_length_stream_as_uncompressed() {
        // A dictionary with an explicit /Length but no /Filter is treated as
        // uncompressed and read verbatim (no zlib decode).
        let body = b"BT (HELLO) Tj ET";
        let pdf = build_stream_object(2, "", body);

        let streams: Vec<_> = stream_scanner(&pdf).collect();
        assert_eq!(streams.len(), 1);
        assert!(!streams[0].is_compressed);
        assert_eq!(streams[0].compressed, body);

        let texts =
            extract_texts_from_stream(streams[0].compressed, streams[0].is_compressed).unwrap();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].0, "HELLO");
    }

    #[test]
    fn test_scanner_skips_non_flate_filter_stream() {
        // A /DCTDecode (JPEG) image stream declares a /Filter we don't support.
        // The scanner skips it entirely rather than mislabeling it uncompressed,
        // so it is never decoded, scanned, or modified.
        let body: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]; // JPEG SOI marker bytes
        let pdf = build_stream_object(3, " /Filter /DCTDecode /Subtype /Image", body);

        let streams: Vec<_> = stream_scanner(&pdf).collect();
        assert!(
            streams.is_empty(),
            "image stream should be skipped, got {}",
            streams.len()
        );
    }

    #[test]
    fn test_scanner_iterates_multiple_mixed_streams() {
        // Two consecutive stream objects (one FlateDecode, one plain) are both
        // yielded, in order, with the correct compression classification.
        let compressed = flate_compress(b"BT (A) Tj ET");
        let mut pdf = Vec::new();
        pdf.extend_from_slice(b"%PDF-1.3\n");
        pdf.extend_from_slice(&build_stream_object(
            1,
            " /Filter [/FlateDecode]",
            &compressed,
        ));
        pdf.extend_from_slice(&build_stream_object(2, "", b"BT (B) Tj ET"));

        let streams: Vec<_> = stream_scanner(&pdf).collect();
        assert_eq!(streams.len(), 2);
        assert!(streams[0].is_compressed);
        assert!(!streams[1].is_compressed);
    }

    #[test]
    fn test_scanner_skips_font_program_stream() {
        // An embedded font program is identified by /Length1 (FontFile/FontFile2)
        // and must never be scanned: its binary body carries no PDF text tokens.
        let body: &[u8] = b"%!PS-AdobeFont binary font data";
        let pdf = build_stream_object(3, " /Length1 12 /Length2 8 /Length3 0", body);

        let streams: Vec<_> = stream_scanner(&pdf).collect();
        assert!(
            streams.is_empty(),
            "font program should be skipped, got {}",
            streams.len()
        );
    }

    #[test]
    fn test_scanner_skips_compressed_font_program_stream() {
        // A Flate-compressed font program (/Length1 + /FlateDecode) must also be
        // skipped: /Length1 is checked before the FlateDecode branch.
        let compressed = flate_compress(b"binary font program bytes");
        let pdf = build_stream_object(6, " /Length1 25 /Filter /FlateDecode", &compressed);

        let streams: Vec<_> = stream_scanner(&pdf).collect();
        assert!(
            streams.is_empty(),
            "compressed font program should be skipped, got {}",
            streams.len()
        );
    }

    #[test]
    fn test_scanner_classifies_name_form_flate_as_compressed() {
        // FlateDecode written as a name (/Filter /FlateDecode) rather than
        // an array is recognized by the unified classifier and decoded.
        let compressed = flate_compress(b"BT (SECRET) Tj ET");
        let pdf = build_stream_object(4, " /Filter /FlateDecode", &compressed);

        let streams: Vec<_> = stream_scanner(&pdf).collect();
        assert_eq!(streams.len(), 1);
        assert!(streams[0].is_compressed);

        let texts =
            extract_texts_from_stream(streams[0].compressed, streams[0].is_compressed).unwrap();
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].0, "SECRET");
    }

    #[test]
    fn test_scanner_discards_unclosed_text_object() {
        // A `BT` never closed by `ET` (typical of non-content binary) must yield
        // no tokens, even when it contains (...)Tj-like patterns.
        let pdf = build_stream_object(5, "", b"BT (SHOULD NOT APPEAR) Tj (NOR THIS) Tj");
        let streams: Vec<_> = stream_scanner(&pdf).collect();
        assert_eq!(streams.len(), 1);
        let texts =
            extract_texts_from_stream(streams[0].compressed, streams[0].is_compressed).unwrap();
        assert!(
            texts.is_empty(),
            "unclosed BT must yield no tokens, got {:?}",
            texts
        );
    }

    /// Returns path to `anonymizer_data/<name>` for offline integration tests.
    /// Place plain PDF fixtures in that directory locally; they are git-ignored.
    fn test_pdf_path(name: &str) -> std::path::PathBuf {
        let manifest = env!("CARGO_MANIFEST_DIR");
        let path = std::path::Path::new(manifest)
            .join("anonymizer_data")
            .join(name);
        assert!(
            path.exists(),
            "Missing test fixture (place the PDF in anonymizer_data/): {}",
            path.display()
        );
        path
    }

    #[test]
    #[ignore = "requires local PDF fixtures in anonymizer_data/"]
    fn test_end_to_end_cash_flow_preserved() {
        let orig_path = test_pdf_path("sample_statement.pdf");

        // Create anonymized output via the library replace function
        let out_path = std::env::temp_dir().join("sample_statement_anonymized_test.pdf");
        crate::anonymizer::replace::replace_pii_smart(&orig_path, &out_path)
            .expect("anonymize failed");

        let orig_bytes = read_pdf(&orig_path).expect("read orig");
        let out_bytes = read_pdf(&out_path).expect("read out");

        fn extract_section(pdf_bytes: &[u8]) -> Option<Vec<String>> {
            for stream in stream_scanner(pdf_bytes) {
                if let Ok(caps) = extract_texts_from_stream(stream.compressed, stream.is_compressed)
                {
                    if let Some(pos) = caps.iter().position(|(s, _st, _en)| {
                        s.trim().eq_ignore_ascii_case("CASH FLOW ACTIVITY BY DATE")
                    }) {
                        let mut sec = Vec::new();
                        for cap in caps.iter().skip(pos) {
                            sec.push(cap.0.clone());
                            if cap.0.to_uppercase().contains("NET CREDITS") {
                                return Some(sec);
                            }
                        }
                        return Some(sec);
                    }
                }
            }
            None
        }

        let orig_sec = extract_section(&orig_bytes).expect("orig sec");
        let out_sec = extract_section(&out_bytes).expect("out sec");

        assert!(orig_sec
            .iter()
            .any(|s| s.trim().eq_ignore_ascii_case("CASH FLOW ACTIVITY BY DATE")));
        assert!(orig_sec
            .iter()
            .any(|s| s.to_uppercase().contains("NET CREDITS")));
        assert_eq!(
            orig_sec, out_sec,
            "CASH FLOW section changed after anonymize"
        );

        let _ = std::fs::remove_file(&out_path);
    }

    #[test]
    #[ignore = "requires local PDF fixtures in anonymizer_data/"]
    fn test_extract_integration_investments() {
        // Integration-style test: open the sample PDF from the repo and
        // assert that our extractor finds the visible "INVESTMENTS" string
        // in at least one stream. This makes the regression explicit.
        let pdf_path = test_pdf_path("sample_statement.pdf");
        let pdf_bytes = match read_pdf(&pdf_path) {
            Ok(b) => b,
            Err(e) => panic!("Failed to read sample PDF: {}", e),
        };

        let mut found = false;
        for stream in stream_scanner(&pdf_bytes) {
            let compressed = stream.compressed;
            if let Ok(texts) = extract_texts_from_stream(compressed, stream.is_compressed) {
                for (s, _st, _en) in texts.iter() {
                    if s.to_uppercase().contains("INVESTMENTS") {
                        found = true;
                        break;
                    }
                }
            }
            if found {
                break;
            }
        }

        assert!(
            found,
            "Extractor did not find 'INVESTMENTS' in sample PDF streams"
        );
    }

    #[test]
    #[ignore = "requires local PDF fixtures in anonymizer_data/"]
    fn test_indexed_list_output_consistency() {
        use std::collections::HashMap;

        let pdf_path = test_pdf_path("sample_statement.pdf");
        let pdf_bytes = match read_pdf(&pdf_path) {
            Ok(b) => b,
            Err(e) => panic!("Failed to read sample PDF: {}", e),
        };

        // preserved tokens that should remain literal in the output
        let preserved = ["CLIENT STATEMENT", "For the Period"];

        let mut global_map: HashMap<String, usize> = HashMap::new();
        let mut next_idx: usize = 0;

        // First pass: build global mapping of non-preserved strings to indices
        for stream in stream_scanner(&pdf_bytes) {
            let caps = match extract_texts_from_stream(stream.compressed, stream.is_compressed) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for (s, _st, _en) in caps {
                let key = s.clone();
                if preserved.iter().any(|p| key.starts_with(p)) {
                    continue; // preserve
                }
                if !global_map.contains_key(&key) {
                    global_map.insert(key.clone(), next_idx);
                    next_idx += 1;
                }
            }
        }

        // We expect the INVESTMENTS phrase to be present in the map and thus indexed
        let investments_key = global_map
            .keys()
            .find(|k| k.to_uppercase().contains("INVESTMENTS"));
        assert!(
            investments_key.is_some(),
            "INVESTMENTS was not indexed in the global map"
        );

        // Second pass: ensure consistent replacement (same string -> same index)
        for stream in stream_scanner(&pdf_bytes) {
            let caps = match extract_texts_from_stream(stream.compressed, stream.is_compressed) {
                Ok(c) => c,
                Err(_) => continue,
            };
            for (s, _st, _en) in caps {
                let key = s.clone();
                if preserved.iter().any(|p| key.starts_with(p)) {
                    // preserved tokens must remain textual
                    assert!(preserved.iter().any(|p| key.starts_with(p)));
                } else {
                    let idx = global_map.get(&key).expect("Missing mapping for token");
                    // index must be stable and within range
                    assert!(*idx < next_idx);
                }
            }
        }
    }

    #[test]
    #[ignore = "requires local PDF fixtures in anonymizer_data/"]
    fn test_anonymized_output_matches_expected() {
        // Run the anonymizer on the input PDF and compare extracted text
        // tokens from the result with those from the expected anonymized PDF.
        let input_path = test_pdf_path("sample_statement.pdf");
        let expected_path = test_pdf_path("sample_statement_anonymized.pdf");

        // Run anonymizer
        let actual_path = std::env::temp_dir().join("test_anonymized_output_cmp.pdf");
        crate::anonymizer::replace::replace_pii_smart(&input_path, &actual_path)
            .expect("anonymize failed");

        // Extract all text tokens from both PDFs
        fn all_tokens(pdf_path: &std::path::Path) -> Vec<Vec<String>> {
            let data = read_pdf(pdf_path).expect("read pdf");
            let mut result = Vec::new();
            for stream in stream_scanner(&data) {
                if let Ok(caps) = extract_texts_from_stream(stream.compressed, stream.is_compressed)
                {
                    let texts: Vec<String> = caps
                        .into_iter()
                        .map(|(s, _, _)| s)
                        // Filter out non-printable binary junk to allow stable comparison
                        .filter(|s| {
                            s.chars()
                                .all(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
                        })
                        .collect();
                    if !texts.is_empty() {
                        result.push(texts);
                    }
                }
            }
            result
        }

        let expected_tokens = all_tokens(&expected_path);
        let actual_tokens = all_tokens(&actual_path);

        // We compare as many streams as we have in the older baseline
        for (i, (exp, act)) in expected_tokens.iter().zip(actual_tokens.iter()).enumerate() {
            assert_eq!(
                exp, act,
                "Stream {} tokens differ.\nExpected: {:?}\nActual:   {:?}",
                i, exp, act
            );
        }

        // Ensure we didn't lose any data (new engine should find AT LEAST as many as old)
        assert!(
            actual_tokens.len() >= expected_tokens.len(),
            "Regression: new engine found fewer text streams ({}) than the old one ({})",
            actual_tokens.len(),
            expected_tokens.len()
        );

        let _ = std::fs::remove_file(&actual_path);
    }

    #[test]
    #[ignore = "requires local PDF fixtures in anonymizer_data/"]
    fn test_pdf_structure_skeleton_integrity() {
        // Ultimate integrity test: verify that outside of the text literals (...),
        // every single byte of the PDF stream (operators, coordinates, etc.)
        // remains exactly the same.
        let input_path = test_pdf_path("sample_statement.pdf");
        let actual_path = std::env::temp_dir().join("integrity_skeleton_test.pdf");
        crate::anonymizer::replace::replace_pii_smart(&input_path, &actual_path)
            .expect("anonymize failed");

        let input_data = read_pdf(&input_path).expect("read input");
        let actual_data = read_pdf(&actual_path).expect("read actual");

        fn get_stream_skeleton(stream_data: &[u8]) -> Vec<u8> {
            let mut decoder = ZlibDecoder::new(stream_data);
            let mut decompressed = Vec::new();
            let _ = decoder.read_to_end(&mut decompressed);

            let mut skeleton = Vec::with_capacity(decompressed.len());
            let mut i = 0;
            while i < decompressed.len() {
                if decompressed[i] == b'(' {
                    // Skip everything inside literal until matching )
                    skeleton.push(b'(');
                    skeleton.push(b'*'); // Placeholder for any text
                    skeleton.push(b')');

                    let mut depth = 1;
                    i += 1;
                    while i < decompressed.len() && depth > 0 {
                        if decompressed[i] == b'\\' && i + 1 < decompressed.len() {
                            i += 2;
                            continue;
                        }
                        if decompressed[i] == b'(' {
                            depth += 1;
                        } else if decompressed[i] == b')' {
                            depth -= 1;
                        }
                        i += 1;
                    }
                } else {
                    skeleton.push(decompressed[i]);
                    i += 1;
                }
            }
            skeleton
        }

        let input_streams: Vec<_> = stream_scanner(&input_data).collect();
        let actual_streams: Vec<_> = stream_scanner(&actual_data).collect();

        assert_eq!(
            input_streams.len(),
            actual_streams.len(),
            "Stream count changed!"
        );

        for (i, (in_s, act_s)) in input_streams.iter().zip(actual_streams.iter()).enumerate() {
            let skel_in = get_stream_skeleton(in_s.compressed);
            let skel_act = get_stream_skeleton(act_s.compressed);

            assert_eq!(
                skel_in, skel_act,
                "Integrity violation in stream {}: non-textual bytes were modified!",
                i
            );
        }

        let _ = std::fs::remove_file(&actual_path);
    }

    #[test]
    #[ignore = "requires local PDF fixtures in anonymizer_data/"]
    fn test_font_program_streams_not_modified() {
        // Embedded font programs (/Length1) must be byte-for-byte identical after
        // anonymization. Guards against the scanner ever treating coincidental
        // byte patterns in binary font data as text tokens.
        let input_path = test_pdf_path("sample_statement.pdf");
        let out_path = std::env::temp_dir().join("font_integrity_test.pdf");
        crate::anonymizer::replace::replace_pii_smart(&input_path, &out_path)
            .expect("anonymize failed");

        let inp = read_pdf(&input_path).expect("read input");
        let out = read_pdf(&out_path).expect("read output");
        assert_eq!(inp.len(), out.len(), "file length changed");

        let re = regex::bytes::Regex::new(
            r"(?s)\d+\s+\d+\s+obj\s*<<[^>]*?/Length\s+(\d+)[^>]*?/Length1\s+\d+[^>]*?>>\s*stream\r?\n",
        )
        .unwrap();
        let mut checked = 0;
        for caps in re.captures_iter(&inp) {
            let whole = caps.get(0).unwrap();
            let len: usize = std::str::from_utf8(caps.get(1).unwrap().as_bytes())
                .unwrap()
                .parse()
                .unwrap();
            let start = whole.end();
            assert_eq!(
                &inp[start..start + len],
                &out[start..start + len],
                "embedded font program bytes changed after anonymize"
            );
            checked += 1;
        }
        assert!(
            checked > 0,
            "expected at least one /Length1 font program in fixture"
        );
        let _ = std::fs::remove_file(&out_path);
    }
}
