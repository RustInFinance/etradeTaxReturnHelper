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

    pub fn run_gui() {
        log::info!("Starting GUI");

        const WIND_SIZE_X: i32 = 800;
        const WIND_SIZE_Y: i32 = 600;
        const DOCUMENTS_REL_WIDTH: f64 = 0.2;
        const TRANSACTIONS_REL_WIDTH: f64 = 0.5;
        const SUMMARY_REL_WIDTH: f64 = 0.3;
        const documents_col_width: i32 = (DOCUMENTS_REL_WIDTH * WIND_SIZE_X as f64) as i32;
        const transactions_col_width: i32 = (TRANSACTIONS_REL_WIDTH * WIND_SIZE_X as f64) as i32;
        const summary_col_width: i32 = (SUMMARY_REL_WIDTH * WIND_SIZE_X as f64) as i32;

        let app = app::App::default();
        let mut wind = window::Window::default()
            .with_size(WIND_SIZE_X, WIND_SIZE_Y)
            .center_screen()
            .with_label("eTradeTaxReturnHelper");

        wind.make_resizable(true);

        let mut pack = Pack::new(0, 0, WIND_SIZE_X as i32, WIND_SIZE_Y as i32, "");
        pack.set_type(fltk::group::PackType::Horizontal);

        let mut pack1 = Pack::new(0, 0, documents_col_width, 300, "");
        pack1.set_type(fltk::group::PackType::Vertical);
        let mut frame1 = Frame::new(0, 0, documents_col_width, 30, "Documents");
        frame1.set_frame(FrameType::EngravedFrame);
        let mut browser = MultiBrowser::new(0, 30, documents_col_width, 270, "");
        browser.add("Document1");
        browser.add("Document2");

        pack1.end();

        let mut pack2 = Pack::new(0, 0, transactions_col_width, 300, "");
        pack2.set_type(fltk::group::PackType::Vertical);
        let mut frame2 = Frame::new(0, 0, transactions_col_width, 30, "Transactions");
        frame2.set_frame(FrameType::EngravedFrame);

        let mut buffer = TextBuffer::default();
        buffer.set_text("
 DIV TRANSACTION date: 2022-03-01, gross: $698.25, tax_us: $104.74, exchange_rate: 4.1965 , exchange_rate_date: 2022-02-28\n
 DIV TRANSACTION date: 2022-06-01, gross: $767.23, tax_us: $115.08, exchange_rate: 4.2651 , exchange_rate_date: 2022-05-31\n
 DIV TRANSACTION date: 2022-09-01, gross: $827.46, tax_us: $124.12, exchange_rate: 4.736 , exchange_rate_date: 2022-08-31\n
 DIV TRANSACTION date: 2022-12-01, gross: $874.54, tax_us: $131.18, exchange_rate: 4.5066 , exchange_rate_date: 2022-11-30\n
 SOLD TRANSACTION trade_date: 2022-04-11, settlement_date: 2022-04-13, acquisition_date: 2013-04-24, net_income: $46.9,  cost_basis: 0, exchange_rate_settlement: 4.2926 , exchange_rate_settlement_date: 2022-04-12, exchange_rate_acquisition: 3.1811 , exchange_rate_acquisition_date: 2013-04-23\n
 SOLD TRANSACTION trade_date: 2022-05-02, settlement_date: 2022-05-04, acquisition_date: 2015-08-19, net_income: $43.67,  cost_basis: 24.258, exchange_rate_settlement: 4.4454 , exchange_rate_settlement_date: 2022-05-02, exchange_rate_acquisition: 3.7578 , exchange_rate_acquisition_date: 2015-08-18
");

        let mut tdisplay = TextDisplay::new(0, 30, transactions_col_width, 270, "");
        tdisplay.set_buffer(buffer);

        pack2.end();

        let mut pack3 = Pack::new(0, 0, summary_col_width, 300, "");
        pack3.set_type(fltk::group::PackType::Vertical);
        let mut frame3 = Frame::new(0, 0, summary_col_width, 30, "Summary");
        frame3.set_frame(FrameType::EngravedFrame);

        let mut buffer = TextBuffer::default();
        buffer.set_text("===> (DYWIDENDY) ZRYCZALTOWANY PODATEK: 2671.89 PLN\n===> (DYWIDENDY) PODATEK ZAPLACONY ZAGRANICA: 2109.38 PLN\n===> (SPRZEDAZ AKCJI) PRZYCHOD Z ZAGRANICY: 395.45 PLN\n===> (SPRZEDAZ AKCJI) KOSZT UZYSKANIA PRZYCHODU: 91.16 PLN");

        let mut sdisplay = TextDisplay::new(0, 30, summary_col_width, 270, "");
        sdisplay.set_buffer(buffer);

        pack3.end();

        pack.end();

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

        app.run().unwrap();
        while app.wait() {
            // handle events
        }
    }
}
