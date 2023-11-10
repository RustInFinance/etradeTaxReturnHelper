# etradeTaxReturnHelper
Project that parse e-trade PDF brokerage statements and Gain and Losses documents and compute total gross gain and tax paid in US that are needed for tax return forms out of US.

### Data for Tax form from capital gains (PIT-38 in Poland)
1. Install this program: `cargo install etradeTaxReturnHelper`
2. Download PDF documents from a year you are filling your tax return form for example: `Brokerage Statement <xxx>.pdf`:
    1. Login to e-trade, navigate to [Documents/Brokerage Statements](https://edoc.etrade.com/e/t/onlinedocs/docsearch?doc_type=stmt)
    2. Select date period
    3. Download all `ACCOUNT STATEMENT` documents
3. Run: `etradeTaxReturnHelper <your PDF documents that MAY contains dividends and/or sold transactions e.g. "*.pdf">`

### FAQ
1. How to install this project?
    1. For Windows OS you can download binary (zip archive holding executable) from [releases](https://github.com/jczaja/e-trade-tax-return-pl-helper/releases) webpage. Place executable in the same directory as desired e-trade documents. Open Windows terminal (command prompt or powershell) and type `etradeTaxReturnHelper.exe *.pdf *.xlsx`

    2. For Linux and MacOS you need Rust and Cargo installed and then you can install this project (crate):
            `cargo install etradeTaxReturnHelper`
    3. For Linux where there is no X server or no priviligies to install system dependencies then you could try to install non-GUI version:
           `cargo install  etradeTaxReturnHelper --no-default-features`

2. How does it work?
    Here is a [demo(PL)](https://www.youtube.com/watch?v=Juw3KJ1JdcA)
3. How can I report problem?
   If this project does not work for you e.g. there is crash or data produced does not seem correct then please run it with diagnostic:
    RUST_LOG=info RUST_BACKTRACE=full etradeTaxReturnHelper <your args e.g. PDF and XLSX files> and share it via issues or via my email (see github profile)
4. How can I help?
    1. Issues and Pull Requests are welcomed!
    2. Buy me a coffee at : https://buycoffee.to/jczaja
    3. If you happen to be an employee of Intel Corporation then you could support this project by
     "giving me **recognition**".
