#!/bin/bash
# Arg: YYYY-MM-DD
function GetRate() { 

prev_day=`date -d "$1 1 day ago" +'%F'`
kurs=`curl -X GET "https://api.nbp.pl/api/exchangerates/rates/a/usd/$prev_day/?format=json"`

while [ "$kurs" = "404 NotFound - Not Found - Brak danych" ]; do
prev_day=`date -d "$prev_day 1 day ago" +'%F'`
kurs=`curl -X GET "https://api.nbp.pl/api/exchangerates/rates/a/usd/$prev_day/?format=json"`
done

# Extract value from Json output
kurs=`echo $kurs | cut -d ':' -f 8 | tr -d '}]'`

echo "$kurs # Transaction date: $1. Rate from: $prev_day" 
}


# A day before given date
# Try to get exchange rate
# If good then ok if not then earlier day
echo "# Gross Tax_US exchange_rate"
echo "div_trans = ["
for brokarage_statement in "$@" 
do
	#GetRate $transaction_date
  echo "# Processing: $brokarage_statement" 1>&2
  extraction=/tmp/`cat /dev/urandom | tr -cd 'a-f0-9' | head -c 8`
  touch $extraction
  pdftotext "$brokarage_statement" $extraction
  transaction_date=`cat $extraction | grep -v Dividends | grep -e Dividend | cut -f 1 -d ' '`
  #if empty skip if non-empty then convert data to format YYYY-MM-DD and get exchange rate
  if [ -n "$transaction_date" ]; then
    converted_transaction_date=`date -d"$transaction_date" +'%F'`
    exchange_rate=`GetRate $converted_transaction_date`

    tax_and_gross=`cat $extraction | grep CREDITED -A 4 | grep -Eo '[0-9]+([.][0-9]+)?'`
    readarray -t y <<< "$tax_and_gross"
    echo "${y[1]} ${y[0]} $exchange_rate"
  fi
done
echo "]"

echo "PRZYCHOD=sum(div_trans(:,1) .* div_trans(:,3))" 
echo "PODATEK_ZAPLACONY_ZA_GRANICA=sum(div_trans(:,2) .* div_trans(:,3))"
echo "PODATEK_NALEZNY_W_POLSCE = sum((div_trans(:,1) .* div_trans(:,3)) * 19/100)"
echo "ROZNICA_DO_ZAPLACENIA_W_POLSCE=(PODATEK_NALEZNY_W_POLSCE - PODATEK_ZAPLACONY_ZA_GRANICA)"

