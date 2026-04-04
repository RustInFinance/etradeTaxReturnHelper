// SPDX-FileCopyrightText: 2023-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

#![cfg(feature = "gui")]

pub use crate::logging::ResultExt;
use fltk::{
    app,
    browser::MultiBrowser,
    button::Button,
    dialog,
    enums::{Event, Font, FrameType, Key, Shortcut},
    frame::Frame,
    group::Pack,
    menu::{MenuBar, MenuFlag},
    prelude::*,
    text::{TextBuffer, TextDisplay},
    window,
};

use crate::pl::PL;
use crate::run_taxation;
use rust_decimal::Decimal;
use rust_decimal::RoundingStrategy;

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Copy, Clone)]
pub enum Message {
    ChoiceChanged,
    Changed,
    Execute,
    Open,
    Quit,
    Copy,
    Paste,
}

/// Dummy method for running diagnostic
fn feed_input(browser: &mut MultiBrowser) {
    let docs = [
    "/home/jczaja/e-trade-tax-return-pl-helper/etrade_data/Brokerage Statement - XXXX0848 - 202202.pdf",
    "/home/jczaja/e-trade-tax-return-pl-helper/etrade_data/Brokerage Statement - XXXX0848 - 202203.pdf",
    "/home/jczaja/e-trade-tax-return-pl-helper/etrade_data/Brokerage Statement - XXXX0848 - 202204.pdf",
    "/home/jczaja/e-trade-tax-return-pl-helper/etrade_data/Brokerage Statement - XXXX0848 - 202205.pdf",
    "/home/jczaja/e-trade-tax-return-pl-helper/etrade_data/Brokerage Statement - XXXX0848 - 202206.pdf",
    "/home/jczaja/e-trade-tax-return-pl-helper/etrade_data/G&L_Expanded.xlsx"];

    docs.iter().for_each(|x| browser.add(x));
}

fn create_clear_documents(
    browser: Rc<RefCell<MultiBrowser>>,
    tdisplay: Rc<RefCell<TextDisplay>>,
    sdisplay: Rc<RefCell<TextDisplay>>,
    ndisplay: Rc<RefCell<TextDisplay>>,
    clear_button: &mut Button,
) {
    clear_button.set_callback(move |_| {
        let mut buffer = sdisplay
            .borrow()
            .buffer()
            .expect_and_log("Error: No buffer assigned to Summary TextDisplay");
        let mut nbuffer = ndisplay
            .borrow()
            .buffer()
            .expect_and_log("Error: No buffer assigned to Notes TextDisplay");
        let mut tbuffer = tdisplay
            .borrow()
            .buffer()
            .expect_and_log("Error: No buffer assigned to Transactions TextDisplay");
        let mut filelist = browser.borrow_mut();
        filelist.clear();
        buffer.set_text("");
        tbuffer.set_text("");
        nbuffer.set_text("");
    });
}

fn round_if_needed(value: Decimal, round_per_transaction: bool) -> Decimal {
    if round_per_transaction {
        value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
    } else {
        value
    }
}

fn format_source_display_value(value: Decimal) -> String {
    value.round_dp(8).normalize().to_string()
}

fn format_pln_display_value(value: Decimal, round_per_transaction: bool) -> String {
    if round_per_transaction {
        value.to_string()
    } else {
        value.round_dp(8).normalize().to_string()
    }
}

fn detect_transaction_currency_code(
    transactions: &[etradeTaxReturnHelper::Transaction],
) -> Option<&'static str> {
    let mut detected: Option<&'static str> = None;

    for transaction in transactions {
        let current = match transaction.gross {
            etradeTaxReturnHelper::Currency::USD(_) => "USD",
            etradeTaxReturnHelper::Currency::EUR(_) => "EUR",
            etradeTaxReturnHelper::Currency::PLN(_) => "PLN",
        };

        match detected {
            Some(existing) if existing != current => return None,
            Some(_) => {}
            None => detected = Some(current),
        }
    }

    detected
}

fn format_transaction_category_totals(
    label: &str,
    transactions: &[etradeTaxReturnHelper::Transaction],
    round_per_transaction: bool,
    include_fx_calculation_details: bool,
) -> Option<String> {
    if transactions.is_empty() {
        return None;
    }

    let currency_code = detect_transaction_currency_code(transactions);
    let gross_source_total: Decimal = transactions.iter().map(|t| t.gross.value()).sum();
    let cost_source_total = Decimal::ZERO;
    let net_source_total = gross_source_total - cost_source_total;

    let gross_pln_total: Decimal = transactions
        .iter()
        .map(|t| round_if_needed(t.exchange_rate * t.gross.value(), round_per_transaction))
        .sum();
    let cost_pln_total = Decimal::ZERO;
    let net_pln_total = gross_pln_total - cost_pln_total;

    let mut line = match currency_code {
        Some(code) => format!(
            "TOTAL {label}: gross_amount({code})={}, cost({code})={}, net({code})={}",
            format_source_display_value(gross_source_total),
            format_source_display_value(cost_source_total),
            format_source_display_value(net_source_total)
        ),
        None => format!(
            "TOTAL {label}: source totals unavailable (mixed currencies), net source total omitted"
        ),
    };

    if include_fx_calculation_details {
        line.push_str(
            format!(
                ", gross_amount(PLN)={}, cost(PLN)={}, net(PLN)={}",
                format_pln_display_value(gross_pln_total, round_per_transaction),
                format_pln_display_value(cost_pln_total, round_per_transaction),
                format_pln_display_value(net_pln_total, round_per_transaction)
            )
            .as_str(),
        );
    }

    Some(line)
}

fn format_sold_category_totals(
    label: &str,
    transactions: &[etradeTaxReturnHelper::SoldTransaction],
    round_per_transaction: bool,
    include_fx_calculation_details: bool,
) -> Option<String> {
    if transactions.is_empty() {
        return None;
    }

    let gross_source_total: Decimal = transactions.iter().map(|t| t.income_us + t.fees).sum();
    let cost_source_total: Decimal = transactions.iter().map(|t| t.cost_basis).sum();
    let net_source_total = gross_source_total - cost_source_total;

    let gross_pln_total: Decimal = transactions
        .iter()
        .map(|t| {
            round_if_needed(
                t.exchange_rate_settlement * (t.income_us + t.fees),
                round_per_transaction,
            )
        })
        .sum();
    let cost_pln_total: Decimal = transactions
        .iter()
        .map(|t| {
            round_if_needed(
                etradeTaxReturnHelper::sold_cost_pln(t),
                round_per_transaction,
            )
        })
        .sum();
    let net_pln_total = gross_pln_total - cost_pln_total;

    let mut line = format!(
        "TOTAL {label}: gross_amount(USD)={}, cost(USD)={}, net(USD)={}",
        format_source_display_value(gross_source_total),
        format_source_display_value(cost_source_total),
        format_source_display_value(net_source_total)
    );

    if include_fx_calculation_details {
        line.push_str(
            format!(
                ", gross_amount(PLN)={}, cost(PLN)={}, net(PLN)={}",
                format_pln_display_value(gross_pln_total, round_per_transaction),
                format_pln_display_value(cost_pln_total, round_per_transaction),
                format_pln_display_value(net_pln_total, round_per_transaction)
            )
            .as_str(),
        );
    }

    Some(line)
}

fn create_execute_documents(
    browser: Rc<RefCell<MultiBrowser>>,
    menubar: Rc<RefCell<MenuBar>>,
    tdisplay: Rc<RefCell<TextDisplay>>,
    sdisplay: Rc<RefCell<TextDisplay>>,
    ndisplay: Rc<RefCell<TextDisplay>>,
    execute_button: &mut Button,
) {
    execute_button.set_callback(move |_| {
        let mut buffer = sdisplay
            .borrow()
            .buffer()
            .expect_and_log("Error: No buffer assigned to Summary TextDisplay");
        let mut nbuffer = ndisplay
            .borrow()
            .buffer()
            .expect_and_log("Error: No buffer assigned to Notes TextDisplay");
        let mut tbuffer = tdisplay
            .borrow()
            .buffer()
            .expect_and_log("Error: No buffer assigned to Transactions TextDisplay");
        let mut file_names: Vec<String> = vec![];
        let list_names = browser.borrow();
        log::info!("Processing {} files", list_names.size());
        if list_names.size() == 0 {
            log::info!("No files to process");
            return;
        }
        for i in 1..=list_names.size() {
            let line_content = browser.borrow().text(i);
            match line_content {
                Some(text) => {
                    log::info!("File to be processed: {}", text);
                    file_names.push(text)
                }
                None => {
                    log::error!("Error: No content in Multbrowse line: {i}");
                    nbuffer.set_text("Error: No content in Multbrowse line: {i}");
                }
            }
        }
        buffer.set_text("");
        tbuffer.set_text("");
        nbuffer.set_text("Running...");
        let round_per_transaction = {
            let mb = menubar.borrow();
            mb.find_item("Options/Round per transaction")
                .map(|item| item.value())
                .unwrap_or(false)
        };
        let include_fx_calculation_details = {
            let mb = menubar.borrow();
            mb.find_item("Options/Include FX calculation details")
                .map(|item| item.value())
                .unwrap_or(false)
        };
        let output_totals = {
            let mb = menubar.borrow();
            mb.find_item("Options/Output totals")
                .map(|item| item.value())
                .unwrap_or(false)
        };
        let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(PL {});
        let etradeTaxReturnHelper::TaxCalculationResult {
            gross_interests,
            gross_div,
            tax: tax_div,
            gross_sold,
            cost_sold,
            interests: interests_transactions,
            transactions: div_transactions,
            revolut_dividends_transactions: revolut_transactions,
            sold_transactions,
            revolut_sold_transactions,
            missing_trade_confirmations_warning: _,
        } = match run_taxation(&rd, file_names, false, false, round_per_transaction) {
            Ok(res) => {
                let mut finish_msg = "Finished.\n\n (Double check if generated tax data (Summary) makes sense and then copy it to your tax form)".to_string();
                if let Some(ref tc_warning) = res.missing_trade_confirmations_warning {
                    finish_msg.push_str("\n\n");
                    finish_msg.push_str(tc_warning);
                }
                nbuffer.set_text(&finish_msg);
                res
            }
            Err(err) => {
                nbuffer.set_text(&err);
                panic!("Error: unable to perform taxation");
            }
        };
        let (presentation,warning) = rd.present_result(gross_interests, gross_div, tax_div, gross_sold, cost_sold);
        buffer.set_text(&presentation.join("\n"));
        if let Some(warn_msg) = warning {
            nbuffer.set_text(&warn_msg);
        }
        let mut transactions_strings: Vec<String> = vec![];
        interests_transactions
            .iter()
            .for_each(|x| transactions_strings.push(x.format_to_print("INTERESTS").expect_and_log("Error: Formatting INTERESTS transaction failed")));
        div_transactions
            .iter()
            .for_each(|x| transactions_strings.push(x.format_to_print("DIV").expect_and_log("Error: Formatting DIV transaction failed")));
        revolut_transactions
            .iter()
            .for_each(|x| transactions_strings.push(x.format_to_print("REVOLUT ").expect_and_log("Error: Formatting DIV transaction failed")));
        sold_transactions
            .iter()
            .for_each(|x| {
                transactions_strings.push(
                    x.format_to_print_with_fx_details(
                        "",
                        include_fx_calculation_details,
                        round_per_transaction,
                    ),
                )
            });
        revolut_sold_transactions
            .iter()
            .for_each(|x| {
                transactions_strings.push(x.format_to_print_with_fx_details(
                    "REVOLUT ",
                    include_fx_calculation_details,
                    round_per_transaction,
                ))
            });

        if output_totals {
            let mut total_lines: Vec<String> = vec![];

            if let Some(line) = format_transaction_category_totals(
                "INTERESTS",
                &interests_transactions,
                round_per_transaction,
                include_fx_calculation_details,
            ) {
                total_lines.push(line);
            }
            if let Some(line) = format_transaction_category_totals(
                "DIV",
                &div_transactions,
                round_per_transaction,
                include_fx_calculation_details,
            ) {
                total_lines.push(line);
            }
            if let Some(line) = format_transaction_category_totals(
                "REVOLUT",
                &revolut_transactions,
                round_per_transaction,
                include_fx_calculation_details,
            ) {
                total_lines.push(line);
            }
            if let Some(line) = format_sold_category_totals(
                "SOLD",
                &sold_transactions,
                round_per_transaction,
                include_fx_calculation_details,
            ) {
                total_lines.push(line);
            }
            if let Some(line) = format_sold_category_totals(
                "REVOLUT SOLD",
                &revolut_sold_transactions,
                round_per_transaction,
                include_fx_calculation_details,
            ) {
                total_lines.push(line);
            }

            if !total_lines.is_empty() {
                transactions_strings.push("----------------".to_string());
                transactions_strings.append(&mut total_lines);
            }
        }

        tbuffer.set_text(&transactions_strings.join("\n"));
    });
}

fn create_choose_documents_dialog(
    browser: Rc<RefCell<MultiBrowser>>,
    tdisplay: Rc<RefCell<TextDisplay>>,
    sdisplay: Rc<RefCell<TextDisplay>>,
    ndisplay: Rc<RefCell<TextDisplay>>,
    load_button: &mut Button,
) {
    load_button.set_callback(move |_| {
        let mut chooser = dialog::FileDialog::new(dialog::FileDialogType::BrowseMultiFile);
        let _ = chooser.set_directory(&".");
        chooser.set_filter("*.{pdf,xlsx,csv}");
        chooser.set_title("Choose e-trade documents with transactions (PDF and/or XLSX)");
        chooser.show();
        if let Some(message) = chooser.error_message() {
            if message != "No error" {
                log::info!("Error in chooser: {}", message);
                return;
            }
        }
        let filenames = chooser.filenames();
        log::info!("{} were selected", filenames.len());
        let mut filelist = browser.borrow_mut();
        for filename in filenames {
            filelist.add(&filename.to_string_lossy());
        }
        let mut buffer = sdisplay
            .borrow()
            .buffer()
            .expect_and_log("Error: No buffer assigned to Summary TextDisplay");
        let mut nbuffer = ndisplay
            .borrow()
            .buffer()
            .expect_and_log("Error: No buffer assigned to Notes TextDisplay");
        let mut tbuffer = tdisplay
            .borrow()
            .buffer()
            .expect_and_log("Error: No buffer assigned to Transactions TextDisplay");
        buffer.set_text("");
        tbuffer.set_text("");
        nbuffer.set_text("");
    });
}

pub fn run_gui() {
    log::info!("Starting GUI");

    const WIND_SIZE_X: i32 = 1024;
    const WIND_SIZE_Y: i32 = 768;
    const DOCUMENTS_REL_WIDTH: f64 = 0.2;
    const TRANSACTIONS_REL_WIDTH: f64 = 0.4;
    const SUMMARY_REL_WIDTH: f64 = 0.4;
    const DOCUMENTS_COL_WIDTH: i32 = (DOCUMENTS_REL_WIDTH * WIND_SIZE_X as f64) as i32;
    const TRANSACTIONS_COL_WIDTH: i32 = (TRANSACTIONS_REL_WIDTH * WIND_SIZE_X as f64) as i32;
    const SUMMARY_COL_WIDTH: i32 = (SUMMARY_REL_WIDTH * WIND_SIZE_X as f64) as i32;

    let app = app::App::default();

    let mut wind = window::Window::default()
        .with_size(WIND_SIZE_X, WIND_SIZE_Y)
        .center_screen()
        .with_label("eTradeTaxReturnHelper");

    wind.make_resizable(true);

    let mut menubar = MenuBar::new(0, 0, WIND_SIZE_X, 25, "");
    menubar.add(
        "Options/Round per transaction",
        Shortcut::None,
        MenuFlag::Toggle,
        |_| {},
    );
    menubar.add(
        "Options/Include FX calculation details",
        Shortcut::None,
        MenuFlag::Toggle,
        |_| {},
    );
    menubar.add(
        "Options/Output totals",
        Shortcut::None,
        MenuFlag::Toggle,
        |_| {},
    );
    let menubar = Rc::new(RefCell::new(menubar));

    let mut uberpack = Pack::new(0, 25, WIND_SIZE_X as i32, WIND_SIZE_Y as i32 - 25, "");

    let mut pack = Pack::new(0, 0, WIND_SIZE_X as i32, WIND_SIZE_Y / 2 as i32, "");
    pack.set_type(fltk::group::PackType::Horizontal);

    let mut pack1 = Pack::new(0, 0, DOCUMENTS_COL_WIDTH, 300, "");
    pack1.set_type(fltk::group::PackType::Vertical);
    let mut frame1 = Frame::new(0, 0, DOCUMENTS_COL_WIDTH, 30, "Documents");
    frame1.set_frame(FrameType::EngravedFrame);

    let browser = Rc::new(RefCell::new(MultiBrowser::new(
        0,
        30,
        DOCUMENTS_COL_WIDTH,
        270,
        "",
    )));
    //feed_input(&mut browser.borrow_mut());

    let mut load_button = Button::new(0, 300, DOCUMENTS_COL_WIDTH, 30, "1. Add");
    load_button.set_label_font(Font::HelveticaBold);
    let mut clear_button = Button::new(0, 300, DOCUMENTS_COL_WIDTH, 30, "Remove All");
    clear_button.set_label_font(Font::HelveticaBold);
    pack1.end();

    let mut pack2 = Pack::new(0, 0, TRANSACTIONS_COL_WIDTH, 300, "");
    pack2.set_type(fltk::group::PackType::Vertical);
    let mut frame2 = Frame::new(0, 0, TRANSACTIONS_COL_WIDTH, 30, "Detected Transactions");
    frame2.set_frame(FrameType::EngravedFrame);

    let mut buffer = TextBuffer::default();
    buffer.set_text("");

    let tdisplay = Rc::new(RefCell::new(TextDisplay::new(
        0,
        30,
        TRANSACTIONS_COL_WIDTH,
        270,
        "",
    )));
    tdisplay.borrow_mut().set_buffer(buffer);

    pack2.end();

    let mut pack3 = Pack::new(0, 0, SUMMARY_COL_WIDTH, 300, "");
    pack3.set_type(fltk::group::PackType::Vertical);
    let mut frame3 = Frame::new(
        0,
        0,
        SUMMARY_COL_WIDTH,
        30,
        "Summary (Data for your Tax form)",
    );
    frame3.set_frame(FrameType::EngravedFrame);

    let buffer = TextBuffer::default();

    let sdisplay = Rc::new(RefCell::new(TextDisplay::new(
        0,
        30,
        SUMMARY_COL_WIDTH,
        270,
        "",
    )));
    sdisplay.borrow_mut().set_buffer(buffer);

    let mut execute_button = Button::new(0, 300, SUMMARY_COL_WIDTH, 30, "2. Execute");
    execute_button.set_label_font(Font::HelveticaBold);

    pack3.end();

    pack.end();

    let mut frame4 = Frame::new(0, pack.height(), WIND_SIZE_X, 30, "Notes:");
    frame4.set_frame(FrameType::EngravedFrame);
    let mut buffer = TextBuffer::default();
    buffer.set_text("Hi!\n\n 1. Add your documents \n 2.  Execute calculation");
    let ndisplay = Rc::new(RefCell::new(TextDisplay::new(0, 30, WIND_SIZE_X, 270, "")));
    ndisplay.borrow_mut().set_buffer(buffer);
    ndisplay.borrow_mut().set_text_size(20);

    uberpack.end();

    create_choose_documents_dialog(
        browser.clone(),
        tdisplay.clone(),
        sdisplay.clone(),
        ndisplay.clone(),
        &mut load_button,
    );
    create_clear_documents(
        browser.clone(),
        tdisplay.clone(),
        sdisplay.clone(),
        ndisplay.clone(),
        &mut clear_button,
    );
    create_execute_documents(
        browser.clone(),
        menubar.clone(),
        tdisplay.clone(),
        sdisplay.clone(),
        ndisplay.clone(),
        &mut execute_button,
    );

    wind.handle(move |wind, ev| {
        let mut dnd = false;
        let mut released = false;
        match ev {
            Event::DndEnter => {
                dnd = true;
                true
            }
            Event::DndDrag => true,
            Event::DndRelease => {
                released = true;
                true
            }
            Event::Paste => {
                let files = app::event_text();
                for file in files.split('\n') {
                    browser.borrow_mut().add(file);
                }
                true
            }
            Event::Resize => {
                uberpack.set_size(wind.width(), wind.height());
                frame4.set_size(wind.width(), frame4.height());

                // First column
                pack.set_size(
                    (DOCUMENTS_REL_WIDTH * wind.width() as f64) as i32,
                    wind.height() / 2,
                );
                pack1.set_size(
                    (DOCUMENTS_REL_WIDTH * wind.width() as f64) as i32,
                    pack1.height(),
                );
                frame1.set_size(
                    (DOCUMENTS_REL_WIDTH * wind.width() as f64) as i32,
                    frame1.height(),
                );

                let height = browser.borrow().height();
                browser
                    .borrow_mut()
                    .set_size((DOCUMENTS_REL_WIDTH * wind.width() as f64) as i32, height);

                //Second column
                pack2.set_size(
                    (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                    pack2.height(),
                );
                frame2.set_size(
                    (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                    frame2.height(),
                );
                let ht = tdisplay.borrow().height();
                tdisplay
                    .borrow_mut()
                    .set_size((TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32, ht);

                //Third column
                pack3.set_size(
                    (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                    pack3.height(),
                );
                frame3.set_size(
                    (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                    frame3.height(),
                );
                let height = sdisplay.borrow().height();
                sdisplay.borrow_mut().set_size(
                    (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                    height,
                );
                true
            }

            Event::KeyUp => {
                if app::event_key() == Key::Delete {
                    let mut list = browser.borrow_mut();
                    let list_size = list.size();
                    // Go in descending order to avoid deleting wrong indices
                    log::info!("Removing elements from list of size: {list_size}");
                    for idx in (1..=list_size).rev() {
                        if list.selected(idx) == true {
                            log::info!("Removing element of index: {idx}");
                            list.remove(idx);
                        }
                    }

                    let mut buffer = sdisplay
                        .borrow()
                        .buffer()
                        .expect_and_log("Error: No buffer assigned to Summary TextDisplay");
                    let mut nbuffer = ndisplay
                        .borrow()
                        .buffer()
                        .expect_and_log("Error: No buffer assigned to Notes TextDisplay");
                    let mut tbuffer = tdisplay
                        .borrow()
                        .buffer()
                        .expect_and_log("Error: No buffer assigned to Transactions TextDisplay");
                    buffer.set_text("");
                    tbuffer.set_text("");
                    nbuffer.set_text("");
                }
                true
            }

            _ => false,
        }
    });

    wind.end();
    wind.show();

    app.run().unwrap();
}
