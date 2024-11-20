### Usage
1. Get JSON data with exchange rates. For example USD to PLN throughput year 2024:
```bash
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2024-01-01/2024-10-31/ > myexchangerates.json
```
2. Run program to get rust source code with embedded exchange rates:
```bash
cargo run --features gen_exchange_rates --bin gen_exchange_rates -- --input myexchangerates.json > myexchange_rates.rs
```
3. Copy generated file to etradeTaxReturnHelper source dir and rebuild project

