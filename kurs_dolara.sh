#!/bin/bash
# Arg: YYYY-MM-DD
function GetRate() { 

prev_day=`date -d "$1 1 day ago" +'%F'`
kurs=`curl -X GET "http://api.nbp.pl/api/exchangerates/rates/a/usd/$prev_day/?format=json"`

while [ "$kurs" = "404 NotFound - Not Found - Brak danych" ]; do
prev_day=`date -d "$prev_day 1 day ago" +'%F'`
kurs=`curl -X GET "http://api.nbp.pl/api/exchangerates/rates/a/usd/$prev_day/?format=json"`
done

# Extract value from Json output
kurs=`echo $kurs | cut -d ':' -f 8 | tr -d '}]'`

echo "$kurs # Transaction date: $1. Rate from: $prev_day"
}

# GetRate YYYY-MM-DD

# A day before given date
# Try to get exchange rate
# If good then ok if not then earlier day
for brokarage_statement in "$@" 
do
	#GetRate $transaction_date
  echo "# Processing: $brokarage_statement"
  extraction=/tmp/`cat /dev/urandom | tr -cd 'a-f0-9' | head -c 8`
  touch $extraction
  pdftotext "$brokarage_statement" $extraction
  transaction_date=`cat $extraction | grep -v Dividends | grep -e Dividend | cut -f 1 -d ' '`
  #if empty skip if non-empty then convert data to format YYYY-MM-DD and get exchange rate
  if [ -n "$transaction_date" ]; then
    converted_transaction_date=`date -d"$transaction_date" +'%F'`
    GetRate $converted_transaction_date
  fi
done

