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
for transaction_date in "$@" 
do
	GetRate $transaction_date
done

