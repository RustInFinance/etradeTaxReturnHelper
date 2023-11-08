pub mod gui {

    pub use crate::logging::ResultExt;
    use fltk::{
        app,
        browser::MultiBrowser,
        button::Button,
        dialog,
        enums::{CallbackTrigger, Color, Event, Font, FrameType, Shortcut},
        frame::Frame,
        group::Pack,
        input::MultilineInput,
        menu,
        menu::Choice,
        prelude::*,
        text,
        text::{TextBuffer, TextDisplay},
        window,
    };

    use crate::pl::PL;
    use crate::run_taxation;

    use std::cell::RefCell;
    use std::rc::Rc;

    pub struct MyMenu {
        _menu: menu::SysMenuBar,
    }

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

    impl MyMenu {
        pub fn new(s: &app::Sender<Message>) -> Self {
            let mut menu = menu::SysMenuBar::default().with_size(800, 35);
            menu.set_frame(FrameType::FlatBox);

            menu.add_emit(
                "&File/Open\t",
                Shortcut::Ctrl | 's',
                menu::MenuFlag::Normal,
                *s,
                Message::Open,
            );

            menu.add_emit(
                "&File/Quit\t",
                Shortcut::Ctrl | 'q',
                menu::MenuFlag::Normal,
                *s,
                Message::Quit,
            );

            Self { _menu: menu }
        }
    }

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

        let (s, r) = app::channel::<Message>();
        let _menu = MyMenu::new(&s);

        let mut uberpack = Pack::new(0, 0, WIND_SIZE_X as i32, WIND_SIZE_Y as i32, "");

        let mut pack = Pack::new(0, 0, WIND_SIZE_X as i32, WIND_SIZE_Y as i32, "");
        pack.set_type(fltk::group::PackType::Horizontal);

        let mut pack1 = Pack::new(0, 0, DOCUMENTS_COL_WIDTH, 300, "");
        pack1.set_type(fltk::group::PackType::Vertical);
        let mut frame1 = Frame::new(0, 0, DOCUMENTS_COL_WIDTH, 30, "Documents");
        frame1.set_frame(FrameType::EngravedFrame);

        let mut browser = Rc::new(RefCell::new(MultiBrowser::new(
            0,
            30,
            DOCUMENTS_COL_WIDTH,
            270,
            "",
        )));
        feed_input(&mut browser.borrow_mut());

        pack1.end();

        let mut pack2 = Pack::new(0, 0, TRANSACTIONS_COL_WIDTH, 300, "");
        pack2.set_type(fltk::group::PackType::Vertical);
        let mut frame2 = Frame::new(0, 0, TRANSACTIONS_COL_WIDTH, 30, "Transactions");
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
        let mut frame3 = Frame::new(0, 0, SUMMARY_COL_WIDTH, 30, "Summary");
        frame3.set_frame(FrameType::EngravedFrame);

        let mut buffer = TextBuffer::default();
        buffer.set_text("Hi! Please load your documents and click >>Execute<<");

        let sdisplay = Rc::new(RefCell::new(TextDisplay::new(
            0,
            30,
            SUMMARY_COL_WIDTH,
            270,
            "",
        )));
        sdisplay.borrow_mut().set_buffer(buffer);

        let mut execute_button = Button::new(0, 300, SUMMARY_COL_WIDTH, 30, "Execute");

        pack3.end();

        pack.end();

        let mut frame4 = Frame::new(0, 200, WIND_SIZE_X, 30, "Notes:");
        frame4.set_frame(FrameType::EngravedFrame);
        let mut buffer = TextBuffer::default();
        buffer.set_text("Neither warnings nor errors yet!");
        let ndisplay = Rc::new(RefCell::new(TextDisplay::new(0, 30, WIND_SIZE_X, 270, "")));
        ndisplay.borrow_mut().set_buffer(buffer);

        uberpack.end();

        let sdisplay_cloned = sdisplay.clone();
        let ndisplay_cloned = ndisplay.clone();
        let tdisplay_cloned = tdisplay.clone();
        let browser_cloned = browser.clone();
        execute_button.set_callback(move |_| {
            let mut buffer = sdisplay_cloned
                .borrow()
                .buffer()
                .expect_and_log("Error: No buffer assigned to Summary TextDisplay");
            let mut nbuffer = ndisplay_cloned
                .borrow()
                .buffer()
                .expect_and_log("Error: No buffer assigned to Notes TextDisplay");
            let mut tbuffer = tdisplay_cloned
                .borrow()
                .buffer()
                .expect_and_log("Error: No buffer assigned to Transactions TextDisplay");
            let mut file_names: Vec<String> = vec![];
            let list_names = browser_cloned.borrow();
            log::info!("Processing {} files", list_names.size());
            for i in 1..list_names.size() + 1 {
                let line_content = browser_cloned.borrow().text(i);
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
            let rd: Box<dyn etradeTaxReturnHelper::Residency> = Box::new(PL {});
            let (gross_div, tax_div, gross_sold, cost_sold, div_transactions, sold_transactions) =
                match run_taxation(&rd, file_names) {
                    Ok((gd, td, gs, cs, dts, sts)) => (gd, td, gs, cs, dts, sts),
                    Err(err) => {
                        nbuffer.set_text(&err);
                        panic!("Error: unable to perform taxation");
                    }
                };
            let presentation = rd.present_result(gross_div, tax_div, gross_sold, cost_sold);
            buffer.set_text(&presentation.join("\n"));
            let mut transactions_strings: Vec<String> = vec![];
            div_transactions
                .iter()
                .for_each(|x| transactions_strings.push(x.format_to_print()));
            sold_transactions
                .iter()
                .for_each(|x| transactions_strings.push(x.format_to_print()));
            tbuffer.set_text(&transactions_strings.join("\n"));
        });

        //        let mut status_line = StatusLine::new(0, wind.height() - 30, wind.width(), 30, "");

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
                    // First column
                    pack.set_size(
                        (DOCUMENTS_REL_WIDTH * wind.width() as f64) as i32,
                        pack.height(),
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
                _ => false,
            }
        });

        wind.end();
        wind.show();

        //        let (gross_div, tax_div, gross_sold, cost_sold) = run_taxation(&rd, ).unwrap();

        app.run().unwrap();
    }
}
