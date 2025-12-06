// SPDX-FileCopyrightText: 2024-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

// TODO: Implement PDF generation only with needed data
// TODO: Implement GUI using eGUI
// TODO: Add tests

use clap::Parser;
use lopdf::{
    content::{Content, Operation},
    dictionary, Document, Object, Stream,
};
use regex::Regex;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Input PDF file path
    #[arg(short, long)]
    input: String,

    /// file path output
    #[arg(short, long)]
    output: String,
}

#[allow(dead_code)]
pub fn init_logging_infrastructure() {
    // Make a default logging level: error
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "error")
    }
    simple_logger::SimpleLogger::new().env().init().unwrap();
}

fn text_as_content(text: &str) -> lopdf::Result<Content> {
    // Content stream (one line of text is one line in output PDF)
    let mut content = Content { operations: vec![] };
    let mut y = 750.0;

    for line in text.lines() {
        content.operations.push(Operation::new("BT", vec![])); // Begin Text
        content
            .operations
            .push(Operation::new("Tf", vec!["F1".into(), 12.into()])); // Font
        content
            .operations
            .push(Operation::new("Td", vec![50.into(), y.into()])); // Position
        content
            .operations
            .push(Operation::new("Tj", vec![Object::string_literal(line)])); // Text
        content.operations.push(Operation::new("ET", vec![])); // End Text
        y -= 15.0;
    }

    Ok(content)
}

// Check if there is "CASH FLOW ACTIVITY BY DATE" section
// if positive then extract that section as a content
fn extract_cash_flow_activity_if_present(contents : &mut Vec<Content>, page : String){

    // Check if this page got "CASH FLOW ACTIVITY"
    let start_pattern = r"CASH FLOW ACTIVITY BY DATE";
    let end_pattern = r"CREDITS/(DEBITS)";
    let re = Regex::new(start_pattern).unwrap();
    if let Some(start_idx) = re.find(&page) {
        log::info!("    Page contains {start_pattern}");
        if let Some(rel_end_idx) = page[start_idx.start()..].find(end_pattern) {
            let end_idx = start_idx.start() + rel_end_idx + end_pattern.len();
            // Get a section between "CASH FLOW ACTIVITY BY DATE" and "CREDITS/(DEBITS)".
            let section = &page[start_idx.start()..end_idx];
            // Replace "CASH FLOW ACTIVITY BY DATE" with "CASH_FLOW_ACTIVITY_BY_DATE"
            let section = section.replace("CASH FLOW ACTIVITY BY DATE", "CASH_FLOW_ACTIVITY_BY_DATE").
                replace("Activity Type", "Activity_Type").replace("DIV PAYMENT", "DIV_PAYMENT").
                replace("TREASURY LIQUIDITY FUND", "TREASURY_LIQUIDITY_FUND").
                replace("NET CREDITS/(DEBITS)", "NET_CREDITS/(DEBITS)");

            section.split_whitespace().for_each(|s| {
                log::info!("        Extracted word: {s}");
                let s = s.replace("_", " "); // Revert back to spaces 
                contents.push(text_as_content(&s).expect(&format!("Error processing string: {s}")));
            });
        } else {
            log::warn!("    Page contains {start_pattern} but no {end_pattern}, extracting till end of page");
        }
    } else {
        log::info!("    Page contains no {start_pattern}");
    }
}

fn save_output_document(output_path: &str, contents: Vec<Content>) -> Result<(), std::io::Error> {
    let mut doc = Document::with_version("1.4");

    let mut kids: Vec<Object> = Vec::new();
    // Font
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });

    let pages_id = doc.new_object_id();
    contents.iter().for_each(|content| {
        let content_stream = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

        // Page object
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "Contents" => content_stream,
            "Resources" => dictionary! {
                "Font" => dictionary! {
                    "F1" => font_id,
                }
            },
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
        });

        kids.push(Object::Reference(page_id));
    });

    // Pages root
    let num_pages = kids.len();
    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => kids,
        "Count" => num_pages as i32,
        "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
    });

    // Catalog
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });

    doc.trailer.set("Root", catalog_id);
    doc.save(output_path)?;
    Ok(())
}

fn main() {
    crate::init_logging_infrastructure();

    let args = Args::parse();

    log::info!("Started etradeAnonymizer");
    log::info!("Input PDF: {}", args.input);
    // Load PDF
    let mut doc = Document::load(&args.input).expect("Cannot load PDF file");
    println!(
        "Generating anonymized PDF: {} (output PDF file) based on {} (input PDF file)",
        args.output, args.input
    );

    let num_pages = doc.get_pages().len();
    log::info!("Input PDF is having {} pages", num_pages);

    let first_page = doc
        .extract_text(&[1])
        .expect("Unable to extract first page");
    log::trace!("First page content: {}", first_page);

    // Based on "CLIENT STATEMENT" on the first page, recognize if we are processing expected
    // type of document
    let re = Regex::new(r"CLIENT STATEMENT").unwrap();
    let _ = re.captures(&first_page).expect("\n ERROR: Wrong type of input PDF. You need to pass a E*TRADE account statement document\n");

    // On first page just write "CLIENT STATEMENT"
    let mut contents: Vec<Content> =
        vec![text_as_content("CLIENT STATEMENT").expect("Unable to create Content")];

    // Iterate through pages 2 to num_pages to find
    // CASH FLOW ACTIVITY blocks
    let mut accumulated_pages = String::new();
    for i in 2..=num_pages {
        let current_page = doc
            .extract_text(&[i as u32])
            .expect("Unable to extract page");
        log::trace!("{i} page content: {}", current_page);
        accumulated_pages.push_str(" ");
        accumulated_pages.push_str(&current_page);
    }
    extract_cash_flow_activity_if_present(&mut contents, accumulated_pages);

    // Create output document
    save_output_document(&args.output, contents).expect("Unable to create PDF");
}
