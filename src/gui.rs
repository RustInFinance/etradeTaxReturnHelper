pub mod gui {

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

        const WIND_SIZE_X: i32 = 800;
        const WIND_SIZE_Y: i32 = 600;
        const DOCUMENTS_REL_WIDTH: f64 = 0.2;
        const TRANSACTIONS_REL_WIDTH: f64 = 0.5;
        const SUMMARY_REL_WIDTH: f64 = 0.3;
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

        let mut pack = Pack::new(0, 0, WIND_SIZE_X as i32, WIND_SIZE_Y as i32, "");
        pack.set_type(fltk::group::PackType::Horizontal);

        let mut pack1 = Pack::new(0, 0, DOCUMENTS_COL_WIDTH, 300, "");
        pack1.set_type(fltk::group::PackType::Vertical);
        let mut frame1 = Frame::new(0, 0, DOCUMENTS_COL_WIDTH, 30, "Documents");
        frame1.set_frame(FrameType::EngravedFrame);
        let mut browser = MultiBrowser::new(0, 30, DOCUMENTS_COL_WIDTH, 270, "");
        feed_input(&mut browser);

        pack1.end();

        let mut pack2 = Pack::new(0, 0, TRANSACTIONS_COL_WIDTH, 300, "");
        pack2.set_type(fltk::group::PackType::Vertical);
        let mut frame2 = Frame::new(0, 0, TRANSACTIONS_COL_WIDTH, 30, "Transactions");
        frame2.set_frame(FrameType::EngravedFrame);

        let mut buffer = TextBuffer::default();
        buffer.set_text("");

        let mut tdisplay = TextDisplay::new(0, 30, TRANSACTIONS_COL_WIDTH, 270, "");
        tdisplay.set_buffer(buffer);

        pack2.end();

        let mut pack3 = Pack::new(0, 0, SUMMARY_COL_WIDTH, 300, "");
        pack3.set_type(fltk::group::PackType::Vertical);
        let mut frame3 = Frame::new(0, 0, SUMMARY_COL_WIDTH, 30, "Summary");
        frame3.set_frame(FrameType::EngravedFrame);

        let mut buffer = TextBuffer::default();
        buffer.set_text("");

        let mut sdisplay = TextDisplay::new(0, 30, SUMMARY_COL_WIDTH, 270, "");
        sdisplay.set_buffer(buffer);

        let mut execute_button = Button::new(0, 0, SUMMARY_COL_WIDTH, 0, "Execute");
        //execute_button.emit(s, Message::Execute);

        pack3.end();

        pack.end();

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
                        browser.add(file);
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
                    browser.set_size(
                        (DOCUMENTS_REL_WIDTH * wind.width() as f64) as i32,
                        browser.height(),
                    );

                    //Second column
                    pack2.set_size(
                        (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                        pack2.height(),
                    );
                    frame2.set_size(
                        (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                        frame2.height(),
                    );
                    tdisplay.set_size(
                        (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                        tdisplay.height(),
                    );

                    //Second column
                    pack3.set_size(
                        (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                        pack3.height(),
                    );
                    frame3.set_size(
                        (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                        frame3.height(),
                    );
                    sdisplay.set_size(
                        (TRANSACTIONS_REL_WIDTH * wind.width() as f64) as i32,
                        sdisplay.height(),
                    );
                    true
                }
                _ => false,
            }
        });

        wind.end();
        wind.show();

        //        let (gross_div, tax_div, gross_sold, cost_sold) = run_taxation(&rd, ).unwrap();
        //    execute_button.set_callback(move |_| display.set_label("Hello world"));

        app.run().unwrap();
    }
}
