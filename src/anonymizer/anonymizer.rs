// SPDX-FileCopyrightText: 2024-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

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

/*
fn save_text_as_pdf(text: &str, output_path: &str) -> lopdf::Result<()> {
   let mut doc = Document::with_version("1.4");

    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
   // Font
   let font_id = doc.add_object(dictionary! {
       "Type" => "Font",
       "Subtype" => "Type1",
       "BaseFont" => "Helvetica",
   });

   // Content stream (jedna linia = jedna linia w PDF)
   let mut content = Content { operations: vec![] };
   let mut y = 750.0;
   for line in text.lines() {
       content.operations.push(Operation::new("BT", vec![])); // Begin Text
       content.operations.push(Operation::new("Tf", vec!["F1".into(), 12.into()])); // Font
       content.operations.push(Operation::new("Td", vec![50.into(), y.into()])); // Position
       content.operations.push(Operation::new("Tj", vec![Object::string_literal(line)])); // Text
       content.operations.push(Operation::new("ET", vec![])); // End Text
       y -= 15.0;
   }
   let content_stream = doc.add_object(Stream::new(dictionary! {}, content.encode().unwrap()));

   // Page
   let page_id = doc.add_object(dictionary! {
       "Type" => "Page",
//       "Parent" => lopdf::Object::Reference(2),
    "Parent" => Object::Reference(pages_id),
       "Contents" => content_stream,
       "Resources" => dictionary! {
           "Font" => dictionary! {
               "F1" => font_id,
           }
       }
   });

   // Pages root
   let pages_id = doc.add_object(dictionary! {
       "Type" => "Pages",
       "Kids" => vec![page_id.into()],
       "Count" => 1,
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
*/
fn main() {
    let args = Args::parse();

    println!("Input PDF: {}", args.input);
    /*
        // Load PDF
        let mut doc = Document::load(&args.input).expect("Cannot load PDF file");
        println!("Output PDF: {}", args.output);
        println!("Replace owner");
        let first_page = doc.extract_text(&[1]).expect("Unable to extract first page");
        println!("First page content: {}", first_page);

        // Next substring after "STATEMENT FOR:" is "\n<Name of owner>\n"
        let re = Regex::new(r"FOR:\n([^\n]+)\n").unwrap();
        let caps = re.captures(&first_page).unwrap();
        let name = &caps[1].trim();
        println!("OWNER: '{}'", name);
    */
    //        let new_content = content.replace("JACEK CZAJA", "John Smith");
    //        doc.change_page_content(page_id, new_content.as_bytes().to_vec());
    //    }

    // Zapisz zmodyfikowany PDF
    //    doc.save(&args.output).expect("Nie można zapisać PDF");
    //  save_text_as_pdf(&first_page, &args.output);

    // Save without modification works fine!
    // Save on example.pdf PDF works fine!
    // Save modified statement PDF does not work!
}
