// SPDX-FileCopyrightText: 2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

//! Text listing module for anonymizer.
//!
//! This module provides functionality to extract and list all text tokens from
//! FlateDecode streams in a PDF. Each token is printed with a global index,
//! useful for understanding the structure and content of the PDF before detection/replacement.

use crate::pdf::{extract_texts_from_stream, read_pdf, stream_scanner};
use log::{info, warn};
use std::error::Error;
use std::path::Path;

/// List all text tokens from FlateDecode streams in the PDF at `input_path`.
///
/// Prints each extracted token with a global index to stdout.
/// Logs warnings for streams that fail to decompress or have invalid markers.
///
/// # Arguments
/// * `input_path` - Path to the input PDF file.
///
/// # Returns
/// `Ok(())` on success, or an error if the PDF cannot be read.
pub fn list_texts(input_path: &Path) -> Result<(), Box<dyn Error>> {
    let pdf_data = read_pdf(input_path)?;

    let mut global_text_id = 0;
    for (stream_id, stream) in stream_scanner(&pdf_data).enumerate() {
        if !stream.valid_end_marker {
            warn!(
                "Skipping stream due to end-marker mismatch for object at {}",
                stream.object_start
            );
            continue;
        }
        match extract_texts_from_stream(stream.compressed) {
            Ok(extracted_texts) => {
                info!("stream {} has {} extracted tokens", stream_id, extracted_texts.len());
                for txt in extracted_texts.iter() {
                    println!("  [{}] {}", global_text_id, txt);
                    global_text_id+=1;
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
    Ok(())
}
