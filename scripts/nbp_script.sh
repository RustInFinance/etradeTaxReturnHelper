# SPDX-FileCopyrightText: 2024-2025 RustInFinance
# SPDX-License-Identifier: BSD-3-Clause

#!/bin/bash
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2025-01-01/2025-12-31/ > rates-2025.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2024-01-01/2024-12-31/ > rates-2024.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2023-01-01/2023-12-31/ > rates-2023.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2022-01-01/2022-12-31/ > rates-2022.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2021-01-01/2021-12-31/ > rates-2021.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2020-01-01/2020-12-31/ > rates-2020.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2019-01-01/2019-12-31/ > rates-2019.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2018-01-01/2018-12-31/ > rates-2018.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2017-01-01/2017-12-31/ > rates-2017.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2016-01-01/2016-12-31/ > rates-2016.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2015-01-01/2015-12-31/ > rates-2015.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2014-01-01/2014-12-31/ > rates-2014.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2013-01-01/2013-12-31/ > rates-2013.json
curl https://api.nbp.pl/api/exchangerates/rates/a/usd/2012-01-01/2012-12-31/ > rates-2012.json

curl https://api.nbp.pl/api/exchangerates/rates/a/eur/2023-01-01/2023-12-31/ > rates-eur-2023.json
curl https://api.nbp.pl/api/exchangerates/rates/a/eur/2024-01-01/2024-12-31/ > rates-eur-2024.json
curl https://api.nbp.pl/api/exchangerates/rates/a/eur/2025-01-01/2025-12-31/ > rates-eur-2025.json

cargo run --features gen_exchange_rates --bin gen_exchange_rates -- --input rates-2025.json --input rates-2023.json --input rates-2024.json --input rates-2022.json --input rates-2021.json --input rates-2020.json --input rates-2019.json --input rates-2018.json --input rates-2017.json --input rates-2016.json --input rates-2015.json --input rates-2014.json --input rates-2013.json --input rates-2012.json --input rates-eur-2023.json --input rates-eur-2024.json --input rates-eur-2025.json > nbp.rs 
rm rates-2012.json rates-2013.json rates-2014.json rates-2015.json rates-2016.json rates-2017.json rates-2018.json rates-2019.json rates-2020.json rates-2021.json rates-2022.json rates-2023.json rates-2024.json rates-2025.json rates-eur-2023.json rates-eur-2024.json rates-eur-2025.json
