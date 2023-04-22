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
    1. For Windows OS you can download binary (zip archive holding executable) from [releases](https://github.com/jczaja/e-trade-tax-return-pl-helper/releases) webpage. Place executable in the same directory as desired e-trade documents. Open Windows PowerShell and type `.\etradeTaxReturnHelper.exe (dir *.pdf -n)`
      
    2. For Linux you need Rust and Cargo installed and then you can install this project (crate):
            `cargo install etradeTaxReturnHelper` 

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

### License
BSD 3-Clause License

Copyright (c) 2022, Jacek Czaja
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

* Redistributions of source code must retain the above copyright notice, this
  list of conditions and the following disclaimer.

* Redistributions in binary form must reproduce the above copyright notice,
  this list of conditions and the following disclaimer in the documentation
  and/or other materials provided with the distribution.

* Neither the name of the copyright holder nor the names of its
  contributors may be used to endorse or promote products derived from
  this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
