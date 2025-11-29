# etradeAnonymizer

Minimal Rust tool for:
- Detecting personally identifiable information (PII) tokens in tightly structured PDF FlateDecode streams.
- Emitting a shell-friendly replace command line.
- Applying replacement strings while preserving original stream size (padding when needed).

## Usage

Detect mode (prints a replacement command suggestion):
```
cargo run --bin etradeAnonymizer -- detect statement.pdf
```

Replace mode (apply explicit replacements):
```
cargo run --bin etradeAnonymizer -- replace input.pdf output.pdf "JAN KOWALSKI" "XXXXX XXXXXXXX"
```

You can chain multiple pairs:
```
cargo run --bin etradeAnonymizer -- replace in.pdf out.pdf "A" "X" "B" "Y"
```

## Build & Test
```
cargo build --release --bin etradeAnonymizer
cargo test --bin etradeAnonymizer
```

Resulting binary: `target/release/etradeAnonymizer`.

## Design Notes
- Strict PDF header (`%PDF-1.3\n`) enforcement; unsupported PDFs are skipped gracefully. This is for simplicity.
- Only FlateDecode streams with explicit `/Length` are processed as described below.
- Replacement recompresses; if no level fits original size, original compressed stream is kept.

### Why Padding? (Architecture Note)
This tool avoids full PDF parsing and rebuilding. Instead, it modifies streams **in-place**.
- PDF files rely on a Cross-Reference (XREF) table that stores the byte offset of every object.
- If we changed the length of a stream object, all subsequent object offsets would shift, invalidating the XREF table.
- To avoid rebuilding the XREF table (which requires full PDF structure understanding), we ensure the modified stream is **exactly the same length** as the original.
- We achieve this by recompressing the modified text. If the new compressed data is smaller, we **pad** the remainder with null bytes (`0x00`).
- If the new compressed data is larger than the original (even at best compression), we cannot safely replace it without corrupting the file, so we fall back to keeping the original stream (and warn the user).

### Exact PDF object pattern searched
The tool searches for PDF objects that exactly match the following pattern (both human-readable and via regex):

Human-readable pattern:

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

Regex used in code (PCRE-style):

```
(?s)\d+\s+\d+\s+obj\s*<<\s*/Length\s+(\d+)\s*/Filter\s*\[\s*/FlateDecode\s*\]\s*>>\s*stream\n
```

Only objects matching this pattern will be considered for detection and replacement for simplicity.

## License
See `BSD-3-Clause` in `LICENSES/` directory.

## Disclaimer

Please note: this tool attempts to detect and replace common personally identifiable
information (PII) tokens in tightly structured PDF streams, but there is no guarantee
that all PII will be detected or removed. You must manually review the resulting
file and verify that sensitive information has been removed before sharing or
publishing. The maintainers make reasonable efforts to identify the following categories:

 - First & last name
 - Mailing address (two lines)
 - Account number

These are the only PII categories we explicitly target.

We provide example screenshots showing the text tokens we look for and recommend
verifying manually:

![Detected tokens — first page](../../../assets/first_page.png)

![Detected tokens — third page](../../../assets/third_page.png)