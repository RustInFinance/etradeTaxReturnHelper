use super::pdf::{process_stream, read_pdf, stream_scanner};
use log::{debug, info, warn};
use std::fs::File;
use std::io::Write;

/// Replace occurrences of given `replacements` inside FlateDecode streams of `input_path` and
/// write modified PDF to `output_path`.
///
/// Each replacement is a `(original, replacement)` pair. Compression is retried to fit
/// the original stream size; if impossible, the original compressed stream is preserved.
pub(crate) fn replace_mode(
    input_path: &str,
    output_path: &str,
    replacements: Vec<(String, String)>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Loading: {}", input_path);

    let pdf_data = match read_pdf(input_path) {
        Ok(d) => d,
        Err(_) => return Ok(()), // header or open error already logged
    };

    debug!("PDF Size: {} bytes", pdf_data.len());

    let mut output_data = pdf_data.clone();
    let mut streams_modified = 0;
    let mut streams_total = 0;
    // Aggregate counts per replacement for concise summary
    let mut replacement_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();

    for stream in stream_scanner(&pdf_data) {
        if !stream.valid_end_marker {
            warn!(
                "Skipping stream due to end-marker mismatch for object at {}",
                stream.object_start
            );
            continue;
        }
        streams_total += 1;

        let compressed_data = stream.compressed;
        let data_start = stream.data_start;
        let stream_end = data_start + compressed_data.len();

        // decompress modify recompress to exact same size
        debug!("═══ Stream #{} ═══", streams_total);
        debug!(
            "Position: {}-{} ({} B)",
            data_start,
            stream_end,
            compressed_data.len()
        );
        let (new_compressed_data, stream_replacement_counts) =
            process_stream(compressed_data, &replacements)?;
        // aggregate counts from this stream
        let mut stream_total = 0usize;
        if !stream_replacement_counts.is_empty() {
            for (k, v) in stream_replacement_counts.iter() {
                *replacement_counts.entry(k.clone()).or_insert(0) += *v;
                stream_total += *v;
            }
        }

        if stream_total > 0 {
            streams_modified += 1;
        }

        // write into output data
        for (idx, &byte) in new_compressed_data.iter().enumerate() {
            output_data[data_start + idx] = byte;
        }
        debug!(
            "Compression: {} → {} B",
            compressed_data.len(),
            new_compressed_data.len()
        );

        let padding_len = compressed_data.len() - new_compressed_data.len();
        for idx in new_compressed_data.len()..compressed_data.len() {
            output_data[data_start + idx] = 0x00;
        }

        if padding_len > 0 {
            info!(
                "Applied padding of {} bytes to stream at {}",
                padding_len, data_start
            );
        }
    }

    info!("Saving: {}", output_path);
    File::create(output_path)?.write_all(&output_data)?;

    info!("DONE!");
    info!(
        "Streams: total={} modified={}",
        streams_total, streams_modified
    );
    let replacements_total: usize = replacement_counts.values().sum();
    info!("Replacements total: {}", replacements_total);
    if !replacement_counts.is_empty() {
        info!("Breakdown:");
        for (k, v) in replacement_counts.iter() {
            info!("  '{}' -> {}", k, v);
        }
    }
    info!("File: {}", output_path);

    Ok(())
}
