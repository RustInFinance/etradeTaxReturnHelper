# e-trade-tax-return-pl-helper
Scripts helpful in computing data for tax return form related to financial instruments

### Data for Tax return form (PIT-38)
1) Download PDF documents from a year you are filling your tax return form for example: Brokerage Statement <xxx>.pdf
2) Run program
e-trade_tax_dividends.sh <your PDF documents that MAY contains dividends transaction e.g. "*.pdf"> | octave

### Dependencies
- octave
- pdftotex
- curl

### Tested on:
- Fedora 29

### FAQ
1) How to remove first page from e-trade brokarage statement
Fedora: pdfseparate -f 2 -l 8 <mybrokerage.pdf>  <somename name>%d.pdf
        pdfunite  <some name>%d.pdf <destination file name>.pdf
Ubuntu: pdftk <mybrokerage.pdf> cat 2-8 output   <mystrippedbrokerage.pdf>
2) It does not work for my PDF with error : "panicked at index out of bound"
 It could be that your PDF stripped (removed some pages) and PDF meta data does not correspond to actual number of pages.

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
