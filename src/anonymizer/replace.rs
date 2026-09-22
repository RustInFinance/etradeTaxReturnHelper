// SPDX-FileCopyrightText: 2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

//! String replacement module for anonymizer.
//!
//! This module applies specified text replacements to all FlateDecode streams in a PDF.
//! For each stream, the module:
//! 1. Decompresses the stream data
//! 2. Applies all specified string replacements
//! 3. Recompresses the modified text to the exact original size (with padding if necessary)
//! 4. Writes the modified PDF to the output file
//!
//! The in-place replacement strategy avoids rebuilding the PDF's XREF table,
//! ensuring the output PDF remains valid without full PDF structure parsing.

use super::pdf::{extract_texts_from_decompressed, read_pdf, stream_scanner};
use flate2::write::ZlibEncoder;
use flate2::Compression;
use log::{debug, info, warn};
use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Replace PII using smart anonymization.
///
/// 1. Preserve strings containing 'CLIENT STATEMENT' or 'For the Period'
/// 2. Replace all other strings with stable numeric tokens (0, 1, 2, ...)
/// 3. When 'CASH FLOW ACTIVITY BY DATE' is encountered, preserve all strings until
///    'NET CREDITS/(DEBITS)' is found (inclusive)
///
/// # Arguments
/// * `input_path` - Path to the input PDF file
/// * `output_path` - Path where the anonymized PDF will be written
///
/// # Returns
/// * `Ok(())` on success
/// * `Err` if PDF cannot be read or processed
pub fn replace_pii_smart(
    input_path: &Path,
    output_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Smart anonymization mode");
    info!("Loading: {}", input_path.display());

    let pdf_data = read_pdf(input_path)?;
    debug!("PDF Size: {} bytes", pdf_data.len());

    let mut output_data = pdf_data.clone();
    let mut streams_modified = 0;
    let mut streams_total = 0;
    let mut streams_skipped = 0;
    let mut letter_counter = 0;
    let mut in_cash_flow_section = false;

    for stream in stream_scanner(&pdf_data) {
        if !stream.valid_end_marker {
            warn!(
                "Skipping stream due to end-marker mismatch for object at {}",
                stream.object_start
            );
            streams_skipped += 1;
            continue;
        }
        streams_total += 1;

        let stream_data = stream.compressed;
        let data_start = stream.data_start;
        let is_compressed = stream.is_compressed;

        debug!(
            "═══ Stream #{} (compressed={}) ═══",
            streams_total, is_compressed
        );
        debug!(
            "Position: {}-{} ({} B)",
            data_start,
            data_start + stream_data.len(),
            stream_data.len()
        );

        match process_stream_smart(
            stream_data,
            is_compressed,
            &mut letter_counter,
            &mut in_cash_flow_section,
        ) {
            Ok((new_data, modified)) => {
                // Check size and warn if too large
                if new_data.len() > stream_data.len() {
                    warn!(
                        "Cannot fit modified data: {} > {} bytes",
                        new_data.len(),
                        stream_data.len()
                    );
                    warn!("Skipping modifications for this stream");
                    streams_skipped += 1;
                    continue;
                }

                // Count only streams whose modifications are actually written out.
                if modified {
                    streams_modified += 1;
                }

                // Write new data
                for (idx, &byte) in new_data.iter().enumerate() {
                    output_data[data_start + idx] = byte;
                }

                // Pad remaining space with NULL bytes to maintain the exact original stream
                // length. Preserving the byte length keeps every subsequent object offset (and
                // therefore the XREF table) valid, so we never need to rebuild it. This applies
                // to both compressed and uncompressed streams; the trailing padding sits after
                // the logical end of the stream content and is ignored by PDF readers.
                let padding_len = stream_data.len() - new_data.len();
                for idx in new_data.len()..stream_data.len() {
                    output_data[data_start + idx] = 0x00;
                }

                if padding_len > 0 {
                    debug!("Applied padding of {} bytes to stream", padding_len);
                }

                debug!("Data size: {} → {} B", stream_data.len(), new_data.len());
            }
            Err(e) => {
                warn!("Failed to process stream: {}", e);
                streams_skipped += 1;
            }
        }
    }

    // Warn loudly (output is still produced): a cash-flow section that opened but
    // never closed means every token after it was preserved and may still contain PII.
    if in_cash_flow_section {
        warn!(
            "CASH FLOW section opened but its closing marker was never found; \
             tokens after it were preserved and may not be anonymized"
        );
    }

    info!("Saving: {}", output_path.display());
    File::create(output_path)?.write_all(&output_data)?;

    info!("DONE!");
    info!(
        "Streams: total={} modified={}",
        streams_total, streams_modified
    );
    if streams_skipped > 0 {
        warn!(
            "Skipped {} stream(s); their bytes were left unmodified and may still contain PII",
            streams_skipped
        );
    }
    info!("File: {}", output_path.display());

    Ok(())
}

/// Check if a text should be preserved based on content rules.
fn should_preserve_text(text: &str) -> bool {
    text.contains("CLIENT STATEMENT") || text.contains("For the Period")
}

/// Process a single stream with smart anonymization.
/// Returns (output_data, was_modified).
/// For compressed streams, returns compressed data. For uncompressed, returns raw data.
fn process_stream_smart(
    stream_data: &[u8],
    is_compressed: bool,
    letter_counter: &mut usize,
    in_cash_flow_section: &mut bool,
) -> Result<(Vec<u8>, bool), Box<dyn std::error::Error>> {
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    let original_len = stream_data.len();

    info!(
        "Processing stream len={} compressed={}",
        original_len, is_compressed
    );

    // Decompress if needed
    let mut decompressed = Vec::new();
    if is_compressed {
        let mut decoder = ZlibDecoder::new(stream_data);
        decoder.read_to_end(&mut decompressed)?;
    } else {
        decompressed = stream_data.to_vec();
    }

    debug!("Decompressed: {} B", decompressed.len());

    // Extract texts along with their byte positions in the decompressed stream.
    // Reuse the buffer we just decompressed above to avoid a second decompression.
    let texts = extract_texts_from_decompressed(&decompressed);

    // Build replacement list based on rules
    let mut replacements: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    for (text, start, end) in texts.iter() {
        let current_token_index = *letter_counter;
        *letter_counter += 1;

        // Log token detection for debugging
        debug!(
            "Token[{}] '{}' at {}..{} (in_cash_flow={})",
            current_token_index, text, start, end, in_cash_flow_section
        );

        // Preserve start/end markers of the CASH FLOW section explicitly.
        if text.contains("CASH FLOW ACTIVITY BY DATE") {
            *in_cash_flow_section = true;
            debug!("Entering CASH FLOW section");
            debug!("Preserving token[{}] '{}'", current_token_index, text);
            continue;
        }

        // Preserve the closing marker and then exit the cash flow section.
        if text.contains("NET CREDITS/(DEBITS)") {
            debug!("Preserving token[{}] '{}'", current_token_index, text);
            *in_cash_flow_section = false;
            debug!("Exiting CASH FLOW section");
            continue;
        }

        // Check if we should preserve this text by general rules
        if *in_cash_flow_section || should_preserve_text(text) {
            debug!("Preserving token[{}] '{}'", current_token_index, text);
            continue;
        } else {
            // Replace entire token with its global index number
            let replacement_str = current_token_index.to_string();

            // PDF string literals are stored escaped in the decompressed stream,
            // but our replacement uses only ASCII digits, so converting
            // to bytes is safe here.
            let replacement_bytes = replacement_str.as_bytes().to_vec();
            debug!(
                "Scheduling replacement for token[{}] '{}' -> '{}' ({} bytes)",
                current_token_index,
                text,
                replacement_str,
                replacement_bytes.len()
            );
            replacements.push((*start, *end, replacement_bytes));
        }
    }

    // Apply replacements to decompressed data. Perform in reverse order of
    // positions so earlier replacements do not affect subsequent indices.
    let mut modified_data = decompressed.clone();
    let mut was_modified = false;

    if !replacements.is_empty() {
        // Sort by start descending
        replacements.sort_by_key(|t| std::cmp::Reverse(t.0));
        for (start, end, repl_bytes) in replacements.iter() {
            let s = *start;
            let e = *end;
            // Build new buffer with replacement applied
            let mut new_buf = Vec::with_capacity(modified_data.len() - (e - s) + repl_bytes.len());
            new_buf.extend_from_slice(&modified_data[..s]);
            new_buf.extend_from_slice(repl_bytes);
            new_buf.extend_from_slice(&modified_data[e..]);
            modified_data = new_buf;
            was_modified = true;
            debug!(
                "Applied replacement at {}..{} ({} B)",
                s,
                e,
                repl_bytes.len()
            );
        }
    }

    // Recompress to fit original size (only for compressed streams)
    if was_modified {
        if is_compressed {
            if let Some((compressed, level)) =
                find_fitting_compression(&modified_data, original_len)
            {
                debug!(
                    "Compressed with level {} ({} B <= {} B)",
                    level,
                    compressed.len(),
                    original_len
                );
                return Ok((compressed, true));
            } else {
                warn!(
                    "Cannot fit modified data into original size ({}B); keeping original",
                    original_len
                );
                return Ok((stream_data.to_vec(), false));
            }
        } else {
            // Uncompressed stream - return modified data directly
            return Ok((modified_data, true));
        }
    }

    Ok((stream_data.to_vec(), false))
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

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::ZlibDecoder;
    use std::io::Read;

    /// Helper: run `process_stream_smart` on uncompressed content with fresh state.
    fn process_uncompressed(content: &[u8]) -> (Vec<u8>, bool, usize) {
        let mut counter = 0usize;
        let mut in_cash_flow = false;
        let (out, modified) =
            process_stream_smart(content, false, &mut counter, &mut in_cash_flow).unwrap();
        (out, modified, counter)
    }

    #[test]
    fn test_replaces_pii_and_preserves_marker_uncompressed() {
        // "JAN KOWALSKI" is PII -> replaced with its token index (0).
        // "CLIENT STATEMENT" is a preserved marker -> kept verbatim.
        let content = b"BT (JAN KOWALSKI) Tj (CLIENT STATEMENT) Tj ET";
        let (out, modified, counter) = process_uncompressed(content);

        assert!(modified, "stream with PII should be reported as modified");
        assert_eq!(out, b"BT (0) Tj (CLIENT STATEMENT) Tj ET");
        // Two tokens were scanned, so the global counter advanced by two.
        assert_eq!(counter, 2);
    }

    #[test]
    fn test_preserves_cash_flow_section_then_resumes_replacement() {
        // Everything from "CASH FLOW ACTIVITY BY DATE" to "NET CREDITS/(DEBITS)"
        // (inclusive) must be preserved; tokens after the section are replaced again.
        let content =
            b"BT (CASH FLOW ACTIVITY BY DATE) Tj (9/1/25) Tj (100.00) Tj (NET CREDITS/(DEBITS)) Tj (SECRET) Tj ET";
        let (out, modified, counter) = process_uncompressed(content);

        assert!(modified);
        let out_str = String::from_utf8(out).unwrap();
        // In-section tokens are untouched.
        assert!(out_str.contains("(CASH FLOW ACTIVITY BY DATE)"));
        assert!(out_str.contains("(9/1/25)"));
        assert!(out_str.contains("(100.00)"));
        assert!(out_str.contains("(NET CREDITS/(DEBITS))"));
        // The token after the section ("SECRET") is the 5th token (index 4).
        assert!(out_str.contains("(4) Tj ET"));
        assert!(!out_str.contains("SECRET"));
        assert_eq!(counter, 5);
    }

    #[test]
    fn test_no_change_when_all_tokens_preserved_uncompressed() {
        let content = b"BT (CLIENT STATEMENT) Tj (For the Period) Tj ET";
        let (out, modified, counter) = process_uncompressed(content);

        assert!(!modified, "fully preserved stream must not be modified");
        assert_eq!(out, content, "unmodified stream must be returned verbatim");
        assert_eq!(counter, 2);
    }

    #[test]
    fn test_compressed_stream_roundtrip_replacement() {
        // A real FlateDecode stream: compress a content stream, run it through the
        // compressed path, and verify the replacement survives a decompress round-trip.
        // The original is stored uncompressed (`Compression::none()`) so that the
        // (shorter) modified stream is guaranteed to fit the original size without
        // depending on exact compressor byte counts.
        let content = b"BT (SECRET) Tj (CLIENT STATEMENT) Tj ET";

        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::none());
        encoder.write_all(content).unwrap();
        let compressed = encoder.finish().unwrap();
        let original_len = compressed.len();

        let mut counter = 0usize;
        let mut in_cash_flow = false;
        let (out, modified) =
            process_stream_smart(&compressed, true, &mut counter, &mut in_cash_flow).unwrap();

        assert!(modified);
        // Output must still fit the original compressed size (XREF-preserving invariant).
        assert!(
            out.len() <= original_len,
            "recompressed stream ({}) must fit original size ({})",
            out.len(),
            original_len
        );

        // Decompress and verify: "SECRET" -> token index 0, marker preserved.
        let mut decoder = ZlibDecoder::new(&out[..]);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();
        assert_eq!(decompressed, b"BT (0) Tj (CLIENT STATEMENT) Tj ET");
        assert_eq!(counter, 2);
    }

    #[test]
    fn test_cash_flow_section_stays_open_without_closing_marker() {
        // Opening marker with no closing "NET CREDITS/(DEBITS)": the section stays
        // open, so later tokens are preserved (not anonymized). This is the state
        // that replace_pii_smart surfaces via a warning.
        let content = b"BT (CASH FLOW ACTIVITY BY DATE) Tj (STILL SECRET) Tj ET";
        let mut counter = 0usize;
        let mut in_cash_flow = false;
        let (out, _modified) =
            process_stream_smart(content, false, &mut counter, &mut in_cash_flow).unwrap();

        assert!(
            in_cash_flow,
            "section must remain open when the closing marker is absent"
        );
        // The post-marker token was preserved verbatim (the potential leak the warning flags).
        let out_str = String::from_utf8(out).unwrap();
        assert!(out_str.contains("(STILL SECRET)"));
    }
}
