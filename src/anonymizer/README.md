<!--
SPDX-FileCopyrightText: 2025 RustInFinance
SPDX-License-Identifier: BSD-3-Clause
-->

# etradeAnonymizer

Minimal Rust tool for anonymizing E*TRADE / Morgan Stanley PDF statements by replacing personally identifiable information while preserving calculation-relevant data.

## Usage

Anonymize a PDF (creates `anonymous_statement.pdf` by default):
```
cargo run --bin etradeAnonymizer -- anonymize statement.pdf
```

Specify output file:
```
cargo run --bin etradeAnonymizer -- anonymize input.pdf output.pdf
```

List all text tokens (for debugging):
```
cargo run --bin etradeAnonymizer -- list statement.pdf
```

## Build & Test
```
cargo build --release --bin etradeAnonymizer
# Tests live in the library target (there are no tests in the binary itself):
cargo test --lib anonymizer
```

Resulting binary: `target/release/etradeAnonymizer`.

## Anonymization Strategy

The tool processes PDF FlateDecode streams and applies smart text replacement:

1. **Preserve calculation-relevant strings**:
   - Strings containing `CLIENT STATEMENT` or `For the Period`
   - Everything between `CASH FLOW ACTIVITY BY DATE` and `NET CREDITS/(DEBITS)` (inclusive)

2. **Replace all other text**:
   - The current implementation replaces non-preserved strings with stable numeric tokens (`0`, `1`, `2`, ...).
   - This removes PII while keeping structure and relative token identity for verification and testing.

## Technical Specification (YAGNI Scope)

This tool follows the **YAGNI (You Ain't Gonna Need It)** principle, focusing only on the subset of the PDF standard actually used in the target documents.

### Supported Features
- **PDF Standard:** Basic PDF 1.3 structure.
- **Streams:** `stream` objects compressed with `/FlateDecode` or raw (uncompressed) with an explicit `/Length`.
- **Text Blocks:** Data contained between `BT` (Begin Text) and `ET` (End Text) operators.
- **Text Operators:** Extraction from `(...) Tj` and `[...] TJ` arrays.
- **Unescaping:** Support for standard PDF escape sequences: `\n`, `\r`, `\t`, `\b`, `\f`, `\(`, `\)`, `\\`, and octal sequences `\ddd`.
- **Encoding:** Text treated as ASCII/Latin-1 (internally handled as UTF-8).


## Design Notes
- **Regex-based Discovery:** The tool uses optimized regular expressions to scan the PDF binary for stream object headers. This allows for fast location of data without full PDF structure parsing.
- Strict PDF header (`%PDF-1.3`) enforcement; files with any other header are rejected.
- FlateDecode and uncompressed streams with an explicit `/Length` are processed.
- Replacement recompresses; if no level fits original size, original compressed stream is kept.

## Testing & Development

### Running Tests
Most unit tests run automatically with `cargo test`. Integration tests that require real PDF files are marked `#[ignore]` and will not run on CI.

To run the integration tests locally:
1. Place the plain PDF fixtures (`sample_statement.pdf`, `sample_statement_anonymized.pdf`) in the `anonymizer_data/` directory (git-ignored).
2. Run:
   ```bash
   cargo test --lib anonymizer -- --ignored
   # or without GUI dependencies:
   cargo test --lib anonymizer --no-default-features -- --ignored
   ```

### Known Limitations (YAGNI Scope)
- **Indirect Length Objects:** The scanner currently only supports streams with an explicit numeric `/Length` in the dictionary. It will skip streams where the length is a reference (e.g., `/Length 12 0 R`).
- **Standard PDF Filters:** Only `/FlateDecode` is supported for text extraction. Image streams (e.g., `/DCTDecode`) and font files are intentionally ignored as they do not contain PII in target documents.

### Why Padding? (Architecture Note)
This tool avoids full PDF parsing and rebuilding. Instead, it modifies streams **in-place**.
- PDF files rely on a Cross-Reference (XREF) table that stores the byte offset of every object.
- If we changed the length of a stream object, all subsequent object offsets would shift, invalidating the XREF table.
- To avoid rebuilding the XREF table, we ensure the modified stream is **exactly the same length** as the original.
- If the new compressed data is smaller, we **pad** the remainder with null bytes (`0x00`).
- If the new compressed data is larger than the original, we fall back to keeping the original stream to avoid file corruption.

### Exact PDF object pattern searched
The tool searches for PDF objects that exactly match the following pattern:

```
<number> <number> obj
<<
/Length <number>
/Filter [/FlateDecode]
>>
stream
<exactly Length bytes>
endstream
endobj
```

Note about `list` showing "0 tokens": the command always prints a stream dump header, but the token extractor only reports tokens when it finds PDF text-showing operators (e.g. `Tj`, `TJ`, `\'`, `"`). Non-text streams (images, fonts, etc.) will naturally show "0 tokens".

## License
See `BSD-3-Clause` in `LICENSES/` directory.

## Disclaimer

Please note: this tool attempts to detect and replace everything except the information required for calculation by analyzing tokens in PDF streams that are strictly defined, but there is no guarantee that all PII will be detected or removed. You must manually review the resulting file before sharing it.
