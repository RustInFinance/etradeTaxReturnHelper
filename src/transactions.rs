// SPDX-FileCopyrightText: 2022-2025 RustInFinance
// SPDX-License-Identifier: BSD-3-Clause

use chrono;
use chrono::Datelike;
use polars::prelude::*;
use rust_decimal::dec;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::{BTreeSet, HashMap};

pub use crate::logging::ResultExt;
use crate::{SoldTransaction, Transaction};

/// Check if all interests rate transactions come from the same year
pub fn verify_interests_transactions<T>(transactions: &Vec<(String, T, T)>) -> Result<(), String> {
    let mut trans = transactions.iter();
    let transaction_date = match trans.next() {
        Some((x, _, _)) => x,
        None => {
            log::info!("No interests transactions");
            return Ok(());
        }
    };

    let transaction_year = chrono::NaiveDate::parse_from_str(transaction_date, "%m/%d/%y")
        .map_err(|_| format!("Unable to parse transaction date: \"{transaction_date}\""))?
        .year();
    let mut verification: Result<(), String> = Ok(());
    trans.try_for_each(|(tr_date, _, _)| {
        let tr_year = chrono::NaiveDate::parse_from_str(tr_date, "%m/%d/%y")
            .map_err(|_| format!("Unable to parse transaction date: \"{tr_date}\""))?
            .year();
        if tr_year != transaction_year {
            let msg: &str = "Error:  Statements are related to different years!";
            verification = Err(msg.to_owned());
        }
        Ok::<(), String>(())
    })?;
    verification
}

/// Check if all dividends transaction come from the same year
pub fn verify_dividends_transactions<T>(
    div_transactions: &Vec<(String, T, T, Option<String>)>,
) -> Result<(), String> {
    let mut trans = div_transactions.iter();
    let transaction_date = match trans.next() {
        Some((x, _, _, _)) => x,
        None => {
            log::info!("No Dividends transactions");
            return Ok(());
        }
    };

    let transaction_year = chrono::NaiveDate::parse_from_str(transaction_date, "%m/%d/%y")
        .map_err(|_| format!("Unable to parse transaction date: \"{transaction_date}\""))?
        .year();
    let mut verification: Result<(), String> = Ok(());
    trans.try_for_each(|(tr_date, _, _, _)| {
        let tr_year = chrono::NaiveDate::parse_from_str(tr_date, "%m/%d/%y")
            .map_err(|_| format!("Unable to parse transaction date: \"{tr_date}\""))?
            .year();
        if tr_year != transaction_year {
            let msg: &str = "Error:  Statements are related to different years!";
            verification = Err(msg.to_owned());
        }
        Ok::<(), String>(())
    })?;
    verification
}

pub fn verify_transactions<T>(
    transactions: &Vec<(String, String, T, T, Option<String>)>,
) -> Result<(), String> {
    let mut trans = transactions.iter();
    let transaction_date = match trans.next() {
        Some((_, x, _, _, _)) => x,
        None => {
            log::info!("No revolut sold transactions");
            return Ok(());
        }
    };

    let transaction_year = chrono::NaiveDate::parse_from_str(transaction_date, "%m/%d/%y")
        .map_err(|_| format!("Unable to parse transaction date: \"{transaction_date}\""))?
        .year();
    let mut verification: Result<(), String> = Ok(());
    trans.try_for_each(|(_, tr_date, _, _, _)| {
        let tr_year = chrono::NaiveDate::parse_from_str(tr_date, "%m/%d/%y")
            .map_err(|_| format!("Unable to parse transaction date: \"{tr_date}\""))?
            .year();
        if tr_year != transaction_year {
            let msg: &str = "Error: Statements are related to different years!";
            verification = Err(msg.to_owned());
        }
        Ok::<(), String>(())
    })?;
    verification
}

/// When trade confirmations are available, fees and commissions are extracted
/// and added to cost basis; otherwise we fall back to net income from account
/// statements (ignoring fees, since they are not broken out there).
/// Trade date(T) is used only for matching G&L rows to sold transactions and
/// for matching trade confirmations. Settlement date (S, typically T+1 or T+2,
/// depending on the stock exchange country), is when the rights
/// are reassigned and income is recognized — FX exchange rate for tax is taken
/// from the last business day preceding settlement date (S-1).
pub fn reconstruct_sold_transactions(
    sold_transactions: &Vec<(String, String, i32, Decimal, Decimal, Option<String>)>,
    gains_and_losses: &Vec<(
        String,
        String,
        Decimal,
        Decimal,
        Decimal,
        i32,
        Option<String>,
    )>,
    trade_confirmations: &Vec<(
        String,
        String,
        i32,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    )>,
) -> Result<
    (
        Vec<(
            String,
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )>,
        Option<String>,
    ),
    String,
> {
    #[derive(Clone, Debug)]
    struct SoldTransactionEx {
        trade_date: String,
        settlement_date: String,
        quantity: i32,
        price: Decimal,
        symbol: Option<String>,
        net_amount: Decimal,
        commission: Decimal,
        fee: Decimal,
        has_trade_confirmation: bool,
    }

    type TradeConfirmationRow = (
        String,
        String,
        i32,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    );

    type TradeConfirmationKey = (String, String, i32, Decimal, Option<String>);
    type TradeConfirmationAmounts = (Decimal, Decimal, Decimal, Decimal);

    fn build_sold_transactions_ex(
        sold_transactions: &Vec<(String, String, i32, Decimal, Decimal, Option<String>)>,
        trade_confirmations: &Vec<TradeConfirmationRow>,
    ) -> Result<(Vec<SoldTransactionEx>, Vec<String>), String> {
        fn normalize_short_us_date_key(date: &str) -> String {
            // Only normalize zero-padding for month/day. Keep original month/day field order.
            let raw = date.trim();
            let parts: Vec<&str> = raw.split('/').collect();
            if parts.len() != 3 {
                return raw.to_string();
            }

            let month = parts[0].trim();
            let day = parts[1].trim();
            let year = parts[2].trim();

            let month_norm = if month.len() == 1 {
                format!("0{month}")
            } else {
                month.to_string()
            };
            let day_norm = if day.len() == 1 {
                format!("0{day}")
            } else {
                day.to_string()
            };

            format!("{month_norm}/{day_norm}/{year}")
        }

        fn take_confirmation(
            confirmations_by_key: &mut HashMap<TradeConfirmationKey, Vec<TradeConfirmationAmounts>>,
            key: &TradeConfirmationKey,
            ambiguity_warnings: &mut Vec<String>,
            warning_context: &str,
        ) -> Option<TradeConfirmationAmounts> {
            if let Some(entries) = confirmations_by_key.get_mut(key) {
                if entries.len() > 1 {
                    let candidate_summaries = entries
                        .iter()
                        .map(|(principal, net_amount, commission, fee)| {
                            format!(
                                "(principal: {}, net_amount: {}, commission: {}, fee: {})",
                                principal, net_amount, commission, fee
                            )
                        })
                        .collect::<Vec<String>>()
                        .join(", ");
                    ambiguity_warnings.push(format!(
                        "WARNING: Ambiguous Trade Confirmation match for sold transaction.\n   match_key=(trade_date: {}, settlement_date: {}, quantity: {}, price: {}, symbol: {:?})\n   matched_confirmations: {}\n   candidate_values: {}\n   context: {}\n   Proceeding with deterministic selection, but fees/net amount may be attached to the wrong sold row if multiple confirmations truly share this key.",
                        key.0,
                        key.1,
                        key.2,
                        key.3,
                        key.4,
                        entries.len(),
                        candidate_summaries,
                        warning_context,
                    ));
                }
                return entries.pop();
            }
            None
        }

        let mut confirmations_by_key: HashMap<TradeConfirmationKey, Vec<TradeConfirmationAmounts>> =
            HashMap::new();
        let mut ambiguity_warnings: Vec<String> = vec![];

        for (
            trade_date,
            settlement_date,
            qty,
            price,
            _principal,
            commission,
            fee,
            net_amount,
            symbol,
        ) in trade_confirmations
        {
            let principal_val = *_principal;
            let commission_val = *commission;
            let fee_val = *fee;
            confirmations_by_key
                .entry((
                    normalize_short_us_date_key(trade_date),
                    normalize_short_us_date_key(settlement_date),
                    *qty,
                    *price,
                    symbol.clone(),
                ))
                .or_insert(vec![])
                .push((principal_val, *net_amount, commission_val, fee_val));
        }

        confirmations_by_key.values_mut().for_each(|entries| {
            entries.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        });

        let mut sold_transactions_ex: Vec<SoldTransactionEx> = vec![];
        for (trade_date, settlement_date, qty, price, _income, symbol) in sold_transactions {
            let trade_date_key = normalize_short_us_date_key(trade_date);
            let settlement_date_key = normalize_short_us_date_key(settlement_date);
            let exact_key = (
                trade_date_key.clone(),
                settlement_date_key.clone(),
                *qty,
                *price,
                symbol.clone(),
            );

            // Try to match with symbol first (when TC has symbol), fall back to None-symbol match.
            // Price is included to disambiguate same-day same-quantity sells.
            let confirmation = take_confirmation(
                &mut confirmations_by_key,
                &exact_key,
                &mut ambiguity_warnings,
                "symbol-exact price-aware match",
            )
            .or_else(|| {
                // If no symbol-exact match, try matching with None-symbol match (TC had no symbol info)
                if symbol.is_some() {
                    let none_symbol_key = (
                        trade_date_key.clone(),
                        settlement_date_key.clone(),
                        *qty,
                        *price,
                        None,
                    );
                    take_confirmation(
                        &mut confirmations_by_key,
                        &none_symbol_key,
                        &mut ambiguity_warnings,
                        "price-aware fallback to trade confirmation without symbol",
                    )
                } else {
                    None
                }
            });

            let (net_amount, commission, fee, has_trade_confirmation) = match confirmation {
                Some((_principal, net_amount, commission, fee)) => {
                    (net_amount, commission, fee, true)
                }
                None => (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO, false),
            };

            sold_transactions_ex.push(SoldTransactionEx {
                trade_date: trade_date.clone(),
                settlement_date: settlement_date.clone(),
                quantity: *qty,
                price: *price,
                symbol: symbol.clone(),
                net_amount,
                commission,
                fee,
                has_trade_confirmation,
            });
        }

        for ((trade_date, settlement_date, qty, price, symbol), entries) in confirmations_by_key {
            if !entries.is_empty() {
                return Err(format!(
                    "\n\nERROR: Not all Trade Confirmations could be matched by trade date + settlement date + quantity + price + symbol.\n\
Unmatched confirmation: trade_date={}, settlement_date={}, quantity={}, price={}, symbol={:?}\n",
                    trade_date, settlement_date, qty, price, symbol
                ));
            }
        }

        Ok((sold_transactions_ex, ambiguity_warnings))
    }

    fn sanity_check_pdf_vs_gl_totals(
        sold_transactions: &Vec<(String, String, i32, Decimal, Decimal, Option<String>)>,
        gains_and_losses: &Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )>,
    ) -> Result<(), String> {
        let pdf_total: Decimal = sold_transactions
            .iter()
            .map(|(_, _, _, _, income, _)| *income)
            .sum();
        let gl_total: Decimal = gains_and_losses
            .iter()
            .map(|(_, _, _, _, total_proceeds, _, _)| *total_proceeds)
            .sum();

        let diff = (pdf_total - gl_total).abs();
        let accepted_delta = dec!(0.004999);

        if diff > accepted_delta {
            return Err(format!(
                "\n\nERROR: Sold transactions mismatch between ClientStatement's PDFs and Gain&Losses XLSX.\n\
PDF total proceeds: {pdf_total:.2}\n\
G&L total proceeds: {gl_total:.2}\n\
Difference: {diff:.2}\n\n\
Please verify that all matching PDF account statements and exactly one Gain&Losses XLSX for the same period are selected.\n"
            ));
        }

        Ok(())
    }

    fn sanity_check_trade_confirmations(
        trade_confirmations: &Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )>,
        gains_and_losses: &Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )>,
    ) -> Result<(), String> {
        if trade_confirmations.is_empty() {
            return Ok(());
        }

        let tc_total_net_amount: Decimal = trade_confirmations
            .iter()
            .map(|(_, _, _, _, _, _, _, net_amount, _)| *net_amount)
            .sum();
        let gl_total: Decimal = gains_and_losses
            .iter()
            .map(|(_, _, _, _, total_proceeds, _, _)| *total_proceeds)
            .sum();

        let diff = (tc_total_net_amount - gl_total).abs();
        let accepted_delta = dec!(0.004999);

        if diff > accepted_delta {
            return Err(format!(
                "\n\nERROR: Trade Confirmation totals mismatch with Gain&Losses XLSX.\n\
Trade Confirmation total net amount: {tc_total_net_amount:.2}\n\
G&L total proceeds: {gl_total:.2}\n\
Difference: {diff:.2}\n\n\
Please verify that all Trade Confirmation PDFs and the Gain&Losses XLSX match the same period.\n"
            ));
        }

        log::info!("Trade Confirmation validation passed: net amount total matches G&L proceeds (diff: {diff:.4})");
        Ok(())
    }

    // Ok What do I need.
    // 1. trade date
    // 2. settlement date (exchange rate is taken from T-1 of this date)
    // 3. date of purchase
    // 4. gross income (or net if no confirmations)
    // 5. cost basis
    // 6. fees (when confirmations exist)
    // 7. company symbol (ticker)
    let mut detailed_sold_transactions: Vec<(
        String,
        String,
        String,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    )> = vec![];

    if sold_transactions.len() > 0 && gains_and_losses.is_empty() {
        return Err("\n\nERROR: Sold transaction detected, but corressponding Gain&Losses document is missing. Please download Gain&Losses  XLSX document at:\n
            https://us.etrade.com/etx/sp/stockplan#/myAccount/gainsLosses\n\n".to_string());
    }

    let mut reconstruction_warnings: Vec<String> = vec![];

    if trade_confirmations.is_empty() && !gains_and_losses.is_empty() {
        let warning = "WARNING: No Trade Confirmation PDFs provided.\n   SELL transactions are based on NET amount from Account Statements.\n   Fees and commissions are not separately extracted.\n   For detailed fee breakdown, include Trade Confirmation PDFs.\n  Obtainable at: https://us.etrade.com/etx/pxy/accountdocs#/documents (select 'Trade Confirmation' type)".to_string();
        println!("\n{warning}\n");
        log::info!("No Trade Confirmation PDFs provided; using Account Statement net amounts for sold transactions");
        reconstruction_warnings.push(warning);
    }

    let (sold_transactions_ex, mut ambiguity_warnings) =
        build_sold_transactions_ex(sold_transactions, trade_confirmations)?;
    reconstruction_warnings.append(&mut ambiguity_warnings);

    let mut gain_to_sold_matches: Vec<(usize, usize)> = vec![];

    let mut gains_by_day: HashMap<chrono::NaiveDate, Vec<usize>> = HashMap::new();
    for (
        gain_idx,
        (_acquisition_date, tr_date, _cost_basis, _adjusted_cost_basis, _inc, quantity, _gl_symbol),
    ) in gains_and_losses.iter().enumerate()
    {
        let trade_date = chrono::NaiveDate::parse_from_str(tr_date, "%m/%d/%Y")
            .expect_and_log(&format!("Unable to parse trade date: {tr_date}"));
        if *quantity <= 0 {
            return Err(format!(
                "\n\nERROR: Gain&Losses quantity must be positive for trade date {tr_date}.\n"
            ));
        }
        gains_by_day
            .entry(trade_date)
            .or_insert(vec![])
            .push(gain_idx);
    }

    let mut sold_by_day: HashMap<chrono::NaiveDate, Vec<usize>> = HashMap::new();
    for (sold_idx, sold_ex) in sold_transactions_ex.iter().enumerate() {
        let trade_date_pdf =
            chrono::NaiveDate::parse_from_str(&sold_ex.trade_date, "%m/%d/%y").expect_and_log(
                &format!("Unable to parse trade date: {}", sold_ex.trade_date),
            );
        sold_by_day
            .entry(trade_date_pdf)
            .or_insert(vec![])
            .push(sold_idx);
    }

    /// Assign each G&L row to a sold transaction such that quantities balance exactly.
    ///
    /// Uses DFS backtracking over the candidate assignments, which is O(S^G) in the
    /// worst case (S = sold txns per day, G = G&L rows per day). Largest-quantity-first
    /// ordering prunes heavily, and in practice a single trading day has very few rows
    /// (typically 1-3 sold transactions), so this completes instantly.
    fn solve_day_knapsack(
        gain_indices: &Vec<usize>,
        sold_indices: &Vec<usize>,
        gains_and_losses: &Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )>,
        sold_transactions_ex: &Vec<SoldTransactionEx>,
    ) -> Option<Vec<(usize, usize)>> {
        fn dfs(
            pos: usize,
            ordered_gain_indices: &Vec<usize>,
            sold_indices: &Vec<usize>,
            gain_qty: &HashMap<usize, i32>,
            remaining_per_sold: &mut HashMap<usize, i32>,
            assignments: &mut Vec<(usize, usize)>,
        ) -> bool {
            if pos >= ordered_gain_indices.len() {
                return sold_indices
                    .iter()
                    .all(|s| remaining_per_sold.get(s).map(|r| *r == 0).unwrap_or(false));
            }

            let gain_idx = ordered_gain_indices[pos];
            let qty = *gain_qty.get(&gain_idx).unwrap_or(&0);

            let mut candidate_sold_indices: Vec<usize> = sold_indices
                .iter()
                .copied()
                .filter(|sold_idx| {
                    let rem = *remaining_per_sold.get(sold_idx).unwrap_or(&0);
                    rem >= qty
                })
                .collect();

            candidate_sold_indices.sort_by(|a, b| {
                let ra = *remaining_per_sold.get(a).unwrap_or(&0);
                let rb = *remaining_per_sold.get(b).unwrap_or(&0);
                rb.cmp(&ra)
            });

            for sold_idx in candidate_sold_indices {
                let rem = *remaining_per_sold.get(&sold_idx).unwrap_or(&0);
                remaining_per_sold.insert(sold_idx, rem - qty);
                assignments.push((gain_idx, sold_idx));

                if dfs(
                    pos + 1,
                    ordered_gain_indices,
                    sold_indices,
                    gain_qty,
                    remaining_per_sold,
                    assignments,
                ) {
                    return true;
                }

                assignments.pop();
                remaining_per_sold.insert(sold_idx, rem);
            }

            false
        }

        let mut gain_qty: HashMap<usize, i32> = HashMap::new();
        for gain_idx in gain_indices {
            gain_qty.insert(*gain_idx, gains_and_losses[*gain_idx].5);
        }

        let mut remaining_per_sold: HashMap<usize, i32> = HashMap::new();
        for sold_idx in sold_indices {
            remaining_per_sold.insert(*sold_idx, sold_transactions_ex[*sold_idx].quantity);
        }

        let mut ordered_gain_indices = gain_indices.clone();
        ordered_gain_indices.sort_by(|a, b| {
            let qa = *gain_qty.get(a).unwrap_or(&0);
            let qb = *gain_qty.get(b).unwrap_or(&0);
            qb.cmp(&qa)
        });

        let mut assignments: Vec<(usize, usize)> = vec![];
        if dfs(
            0,
            &ordered_gain_indices,
            sold_indices,
            &gain_qty,
            &mut remaining_per_sold,
            &mut assignments,
        ) {
            Some(assignments)
        } else {
            None
        }
    }

    // Trade confirmation matching is symbol-aware: matching key includes symbol extracted from
    // the trade confirmation PDF. For account statement sold transactions, symbol comes from G&L XLSX
    // ticker column. Matching is strict: all same-day G&L and sold rows must have symbol and match
    // by symbol before quantity allocation.
    let mut all_days: Vec<chrono::NaiveDate> = gains_by_day
        .keys()
        .chain(sold_by_day.keys())
        .cloned()
        .collect();
    all_days.sort();
    all_days.dedup();

    for day in all_days {
        let day_gains = gains_by_day.get(&day).cloned().unwrap_or(vec![]);
        let day_sold = sold_by_day.get(&day).cloned().unwrap_or(vec![]);

        let mut day_gains_by_symbol: HashMap<String, Vec<usize>> = HashMap::new();
        for gain_idx in &day_gains {
            let gain_sym = gains_and_losses[*gain_idx]
                .6
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or(format!(
                    "\n\nERROR: Missing symbol in Gain&Losses row for strict same-day symbol matching.\n\
trade_date: {}\n",
                    day.format("%m/%d/%Y")
                ))?
                .to_string();
            day_gains_by_symbol
                .entry(gain_sym)
                .or_insert(vec![])
                .push(*gain_idx);
        }

        let mut day_sold_by_symbol: HashMap<String, Vec<usize>> = HashMap::new();
        for sold_idx in &day_sold {
            let sold_sym = sold_transactions_ex[*sold_idx]
                .symbol
                .as_ref()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .ok_or(format!(
                    "\n\nERROR: Missing symbol in sold transaction row for strict same-day symbol matching.\n\
trade_date: {}\n",
                    day.format("%m/%d/%Y")
                ))?
                .to_string();
            day_sold_by_symbol
                .entry(sold_sym)
                .or_insert(vec![])
                .push(*sold_idx);
        }

        let mut all_symbols: BTreeSet<String> = BTreeSet::new();
        for sym in day_gains_by_symbol.keys() {
            all_symbols.insert(sym.clone());
        }
        for sym in day_sold_by_symbol.keys() {
            all_symbols.insert(sym.clone());
        }

        for sym in all_symbols {
            let symbol_gains = day_gains_by_symbol.get(&sym).cloned().unwrap_or(vec![]);
            let symbol_sold = day_sold_by_symbol.get(&sym).cloned().unwrap_or(vec![]);

            if symbol_gains.is_empty() {
                let symbol_sold_qty = symbol_sold.iter().fold(0i32, |acc, sold_idx| {
                    acc + sold_transactions_ex[*sold_idx].quantity
                });
                return Err(format!(
                    "\n\nERROR: Missing Gain&Losses entries for symbol/trade-date found in PDF account statements.\n\
trade_date: {}\n\
symbol: {}\n\
PDF quantity: {}\n\
\n\
Please verify that the selected Gain&Losses XLSX covers the same period as the PDF statements.\n",
                    day.format("%m/%d/%Y"),
                    sym,
                    symbol_sold_qty
                ));
            }

            if symbol_sold.is_empty() {
                let symbol_gain_qty = symbol_gains
                    .iter()
                    .fold(0i32, |acc, gain_idx| acc + gains_and_losses[*gain_idx].5);
                return Err(format!(
                    "\n\nERROR: Missing PDF sold transaction entries for symbol/trade-date found in Gain&Losses XLSX.\n\
trade_date: {}\n\
symbol: {}\n\
G&L quantity: {}\n\
\n\
This usually means one or more monthly Account Statement PDFs are missing for the selected period.\n",
                    day.format("%m/%d/%Y"),
                    sym,
                    symbol_gain_qty
                ));
            }

            let symbol_gain_qty = symbol_gains
                .iter()
                .fold(0i32, |acc, gain_idx| acc + gains_and_losses[*gain_idx].5);
            let symbol_sold_qty = symbol_sold.iter().fold(0i32, |acc, sold_idx| {
                acc + sold_transactions_ex[*sold_idx].quantity
            });

            if symbol_gain_qty != symbol_sold_qty {
                return Err(format!(
                    "\n\nERROR: Same-day quantity mismatch between Gain&Losses XLSX and PDF sold transactions for symbol.\n\
trade_date: {}\n\
symbol: {}\n\
G&L total quantity: {}\n\
PDF total quantity: {}\n",
                    day.format("%m/%d/%Y"),
                    sym,
                    symbol_gain_qty,
                    symbol_sold_qty
                ));
            }

            let mut symbol_matches = solve_day_knapsack(
                &symbol_gains,
                &symbol_sold,
                gains_and_losses,
                &sold_transactions_ex,
            )
            .ok_or(format!(
                "\n\nERROR: Unable to allocate Gain&Losses rows into same-day sold transactions by quantity for symbol.\n\
trade_date: {}\n\
symbol: {}\n",
                day.format("%m/%d/%Y"),
                sym
            ))?;

            gain_to_sold_matches.append(&mut symbol_matches);
        }
    }

    gain_to_sold_matches.sort_by_key(|(gain_idx, _)| *gain_idx);

    let mut total_gl_per_sold_index: HashMap<usize, Decimal> = HashMap::new();
    let mut total_gl_qty_per_sold_index: HashMap<usize, i32> = HashMap::new();
    let mut gain_indices_per_sold_index: HashMap<usize, Vec<usize>> = HashMap::new();
    for (gain_idx, sold_index) in &gain_to_sold_matches {
        let gl_inc = gains_and_losses[*gain_idx].4;
        let gl_qty = gains_and_losses[*gain_idx].5;
        *total_gl_per_sold_index
            .entry(*sold_index)
            .or_insert(Decimal::ZERO) += gl_inc;
        *total_gl_qty_per_sold_index.entry(*sold_index).or_insert(0) += gl_qty;
        gain_indices_per_sold_index
            .entry(*sold_index)
            .or_insert(vec![])
            .push(*gain_idx);
    }

    // Precompute proportional allocations per sold transaction and gain row.
    //
    // Why this exists:
    // - Trade confirmations provide exact totals (net amount and total fees) per sold transaction.
    // - A single sold transaction can map to multiple G&L rows (same-day split by quantity).
    //
    // Rounding gotcha:
    // - If we round each proportional row independently (for example to 6 decimal places),
    //   the sum of rounded parts may not equal the original exact total.
    // - This produces visible tails like 10015.840001 in aggregated outputs.
    //
    // What we do instead:
    // - Compute proportional values for all rows except the last one without per-row rounding.
    // - Assign the exact remainder to the last row:
    //   last_net  = sold_net_total  - sum(previous_nets)
    //   last_fee  = sold_fee_total  - sum(previous_fees)
    // - This guarantees exact recomposition of totals per sold transaction.
    let mut proportional_allocations_by_gain_index: HashMap<usize, (Decimal, Decimal)> =
        HashMap::new();
    for (sold_index, gain_indices) in &mut gain_indices_per_sold_index {
        gain_indices.sort_unstable();

        let sold_ex = &sold_transactions_ex[*sold_index];
        if !sold_ex.has_trade_confirmation {
            continue;
        }

        let gl_total_for_sold = *total_gl_per_sold_index
            .get(sold_index)
            .unwrap_or(&Decimal::ZERO);
        let gl_total_qty_for_sold = *total_gl_qty_per_sold_index.get(sold_index).unwrap_or(&0);
        if gl_total_for_sold <= Decimal::ZERO {
            continue;
        }

        let total_fees = sold_ex.commission + sold_ex.fee;
        let mut allocated_net = Decimal::ZERO;
        let mut allocated_fees = Decimal::ZERO;

        for (position, gain_idx) in gain_indices.iter().enumerate() {
            let (proportional_net, proportional_fee) = if position + 1 == gain_indices.len() {
                (
                    sold_ex.net_amount - allocated_net,
                    total_fees - allocated_fees,
                )
            } else {
                let inc = gains_and_losses[*gain_idx].4;
                let qty = gains_and_losses[*gain_idx].5;
                let ratio = if gl_total_qty_for_sold > 0 {
                    Decimal::from(qty) / Decimal::from(gl_total_qty_for_sold)
                } else {
                    // Fallback for legacy/ill-formed rows with missing quantity.
                    inc / gl_total_for_sold
                };
                let proportional_net = sold_ex.net_amount * ratio;
                let proportional_fee = total_fees * ratio;
                allocated_net += proportional_net;
                allocated_fees += proportional_fee;
                (proportional_net, proportional_fee)
            };

            proportional_allocations_by_gain_index
                .insert(*gain_idx, (proportional_net, proportional_fee));
        }
    }

    // iterate through all sold transactions and update it with needed info
    for (gain_idx, sold_index) in gain_to_sold_matches {
        let (acquisition_date, tr_date, cost_basis, _, inc, _quantity, _gl_symbol) =
            &gains_and_losses[gain_idx];

        log::info!("Reconstructing G&L sold transaction: trade date: {tr_date}, acquisition date: {acquisition_date}, cost basis: {cost_basis}, income: {inc}");
        let sold_ex = &sold_transactions_ex[sold_index];
        let settlement_date = &sold_ex.settlement_date;
        let symbol = &sold_ex.symbol;

        let (mut adjusted_income, mut adjusted_cost_basis, mut adjusted_fees) =
            (*inc, *cost_basis, Decimal::ZERO);
        if sold_ex.has_trade_confirmation {
            let gl_total_for_sold = *total_gl_per_sold_index
                .get(&sold_index)
                .unwrap_or(&Decimal::ZERO);
            let gl_total_qty_for_sold = *total_gl_qty_per_sold_index.get(&sold_index).unwrap_or(&0);
            if gl_total_for_sold > Decimal::ZERO {
                let ratio = if gl_total_qty_for_sold > 0 {
                    Decimal::from(gains_and_losses[gain_idx].5)
                        / Decimal::from(gl_total_qty_for_sold)
                } else {
                    *inc / gl_total_for_sold
                };
                let (proportional_net, proportional_fee) = proportional_allocations_by_gain_index
                    .get(&gain_idx)
                    .copied()
                    .unwrap_or((
                        sold_ex.net_amount * ratio,
                        (sold_ex.commission + sold_ex.fee) * ratio,
                    ));

                // adjusted_income is the net proceeds (what the seller receives)
                adjusted_income = proportional_net;
                // fees separately for display
                adjusted_fees = proportional_fee;
                // cost_basis still includes fees for tax purposes
                adjusted_cost_basis = *cost_basis + proportional_fee;

                log::info!(
                    "Applied Trade Confirmation enrichment to sold trade: qty={}, price={:.4}, ratio={}, gross={}, fee={}",
                    sold_ex.quantity,
                    sold_ex.price,
                    ratio,
                    proportional_net + proportional_fee,
                    proportional_fee
                );
            }
        }

        detailed_sold_transactions.push((
            chrono::NaiveDate::parse_from_str(&tr_date, "%m/%d/%Y")
                .expect(&format!("Unable to parse trade date: {tr_date}"))
                .format("%m/%d/%y")
                .to_string(),
            settlement_date.clone(),
            chrono::NaiveDate::parse_from_str(&acquisition_date, "%m/%d/%Y")
                .expect(&format!(
                    "Unable to parse acquisition_date: {acquisition_date}"
                ))
                .format("%m/%d/%y")
                .to_string(),
            adjusted_income,
            adjusted_cost_basis,
            adjusted_fees,
            symbol.clone(),
        ));
    }

    // Seems matching was OK. Double check we have the same totals in all flavors of PDFs and XLSX
    // Doing it only now, not at the beginning, to have a better contextual help on missed match in the logic above.
    sanity_check_pdf_vs_gl_totals(sold_transactions, gains_and_losses)?;
    sanity_check_trade_confirmations(trade_confirmations, gains_and_losses)?;

    let reconstruction_warning = if reconstruction_warnings.is_empty() {
        None
    } else {
        Some(reconstruction_warnings.join("\n\n"))
    };

    Ok((detailed_sold_transactions, reconstruction_warning))
}

pub fn create_detailed_revolut_transactions(
    transactions: Vec<(String, crate::Currency, crate::Currency, Option<String>)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>>,
) -> Result<Vec<Transaction>, &str> {
    let mut detailed_transactions: Vec<Transaction> = Vec::new();

    transactions
        .iter()
        .try_for_each(|(transaction_date, gross, tax, company)| {
            let (exchange_rate_date, exchange_rate) = dates
                [&gross.derive_exchange(transaction_date.clone())]
                .clone()
                .unwrap();

            let transaction = Transaction {
                transaction_date: transaction_date.clone(),
                gross: *gross,
                tax_paid: *tax,
                exchange_rate_date,
                exchange_rate,
                company: company.clone(),
            };

            let msg = transaction.format_to_print("REVOLUT")?;

            println!("{}", msg);
            log::info!("{}", msg);
            detailed_transactions.push(transaction);
            Ok::<(), &str>(())
        })?;
    Ok(detailed_transactions)
}

pub fn create_detailed_interests_transactions(
    transactions: Vec<(String, Decimal, Decimal)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>>,
) -> Result<Vec<Transaction>, &str> {
    let mut detailed_transactions: Vec<Transaction> = Vec::new();
    transactions
        .iter()
        .try_for_each(|(transaction_date, gross_us, tax_us)| {
            let (exchange_rate_date, exchange_rate) = dates
                [&crate::Exchange::USD(transaction_date.clone())]
                .clone()
                .unwrap();

            let transaction = Transaction {
                transaction_date: transaction_date.clone(),
                gross: crate::Currency::USD(*gross_us),
                tax_paid: crate::Currency::USD(*tax_us),
                exchange_rate_date,
                exchange_rate,
                company: None, // No company info when interests are paid on money
            };

            let msg = transaction.format_to_print("INTERESTS")?;

            println!("{}", msg);
            log::info!("{}", msg);
            detailed_transactions.push(transaction);
            Ok::<(), &str>(())
        })?;
    Ok(detailed_transactions)
}

pub fn create_detailed_div_transactions(
    transactions: Vec<(String, Decimal, Decimal, Option<String>)>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>>,
) -> Result<Vec<Transaction>, &str> {
    let mut detailed_transactions: Vec<Transaction> = Vec::new();
    transactions
        .iter()
        .try_for_each(|(transaction_date, gross_us, tax_us, company)| {
            let (exchange_rate_date, exchange_rate) = dates
                [&crate::Exchange::USD(transaction_date.clone())]
                .clone()
                .unwrap();

            let transaction = Transaction {
                transaction_date: transaction_date.clone(),
                gross: crate::Currency::USD(*gross_us),
                tax_paid: crate::Currency::USD(*tax_us),
                exchange_rate_date,
                exchange_rate,
                company: company.clone(),
            };

            let msg = transaction.format_to_print("DIV")?;

            println!("{}", msg);
            log::info!("{}", msg);
            detailed_transactions.push(transaction);
            Ok::<(), &str>(())
        })?;
    Ok(detailed_transactions)
}

//    pub trade_date: String,
//    pub settlement_date: String,
//    pub acquisition_date: String,
//    pub income_us: Decimal,
//    pub cost_basis: Decimal,
//    pub exchange_rate_settlement_date: String,
//    pub exchange_rate_settlement: Decimal,
//    pub exchange_rate_acquisition_date: String,
//    pub exchange_rate_acquisition: Decimal,
pub fn create_detailed_sold_transactions(
    transactions: Vec<(
        String,
        String,
        String,
        Decimal,
        Decimal,
        Decimal,
        Option<String>,
    )>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>>,
) -> Result<Vec<SoldTransaction>, &str> {
    let mut detailed_transactions: Vec<SoldTransaction> = Vec::new();
    transactions.iter().for_each(
        |(trade_date, settlement_date, acquisition_date, income, cost_basis, fees, symbol)| {
            let (exchange_rate_settlement_date, exchange_rate_settlement) = dates
                [&crate::Exchange::USD(settlement_date.clone())]
                .clone()
                .unwrap();
            let (exchange_rate_acquisition_date, exchange_rate_acquisition) = dates
                [&crate::Exchange::USD(acquisition_date.clone())]
                .clone()
                .unwrap();

            let transaction = SoldTransaction {
                trade_date: trade_date.clone(),
                settlement_date: settlement_date.clone(),
                acquisition_date: acquisition_date.clone(),
                income_us: *income,
                cost_basis: *cost_basis,
                fees: *fees,
                exchange_rate_settlement_date,
                exchange_rate_settlement,
                exchange_rate_acquisition_date,
                exchange_rate_acquisition,
                company: symbol.clone(),
            };

            let msg = transaction.format_to_print("");

            println!("{}", msg);
            log::info!("{}", msg);

            detailed_transactions.push(transaction);
        },
    );
    Ok(detailed_transactions)
}

pub fn create_detailed_revolut_sold_transactions(
    transactions: Vec<(
        String,
        String,
        crate::Currency,
        crate::Currency,
        Option<String>,
    )>,
    dates: &std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>>,
) -> Result<Vec<SoldTransaction>, &str> {
    let mut detailed_transactions: Vec<SoldTransaction> = Vec::new();
    transactions.iter().for_each(
        |(acquired_date, sold_date, cost_basis, gross_income, symbol)| {
            // For Revolut transactions sold_date is the transaction date.
            let (exchange_rate_settlement_date, exchange_rate_settlement) = dates
                [&gross_income.derive_exchange(sold_date.clone())]
                .clone()
                .unwrap();
            let (exchange_rate_acquisition_date, exchange_rate_acquisition) = dates
                [&cost_basis.derive_exchange(acquired_date.clone())]
                .clone()
                .unwrap();

            let transaction = SoldTransaction {
                trade_date: sold_date.clone(),
                settlement_date: sold_date.clone(), // Revolut has no separate settlement date
                acquisition_date: acquired_date.clone(),
                income_us: gross_income.value(),
                cost_basis: cost_basis.value(),
                fees: Decimal::ZERO,
                exchange_rate_settlement_date,
                exchange_rate_settlement,
                exchange_rate_acquisition_date,
                exchange_rate_acquisition,
                company: symbol.clone(),
            };

            let msg = transaction.format_to_print("REVOLUT ");

            println!("{}", msg);
            log::info!("{}", msg);

            detailed_transactions.push(transaction);
        },
    );
    Ok(detailed_transactions)
}

// Make a dataframe with
pub(crate) fn create_per_company_report(
    interests: &[Transaction],
    dividends: &[Transaction],
    sold_transactions: &[SoldTransaction],
    revolut_dividends_transactions: &[Transaction],
    revolut_sold_transactions: &[SoldTransaction],
) -> Result<DataFrame, &'static str> {
    // Key: Company Name , Value : (gross_pl, tax_paid_in_us_pl, cost_pl)
    let mut per_company_data: HashMap<Option<String>, (Decimal, Decimal, Decimal)> = HashMap::new();

    let interests_or_dividends = interests
        .iter()
        .chain(dividends.iter())
        .chain(revolut_dividends_transactions.iter());

    interests_or_dividends.for_each(|x| {
        let entry = per_company_data.entry(x.company.clone()).or_insert((
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        ));
        entry.0 += x.exchange_rate * x.gross.value();
        entry.1 += x.exchange_rate * x.tax_paid.value();
        // No cost for dividends being paid
    });

    let sells = sold_transactions
        .iter()
        .chain(revolut_sold_transactions.iter());
    sells.for_each(|x| {
        let entry = per_company_data.entry(x.company.clone()).or_insert((
            Decimal::ZERO,
            Decimal::ZERO,
            Decimal::ZERO,
        ));
        entry.0 += x.income_us * x.exchange_rate_settlement;
        // No tax from sold transactions
        entry.2 += crate::sold_cost_pln(x);
    });

    // Convert my HashMap into DataFrame
    let mut companies: Vec<Option<String>> = Vec::new();
    let mut gross: Vec<f64> = Vec::new();
    let mut tax: Vec<f64> = Vec::new();
    let mut cost: Vec<f64> = Vec::new();
    per_company_data
        .iter()
        .try_for_each(|(company, (gross_pl, tax_paid_in_us_pl, cost_pl))| {
            log::info!(
                "Company: {:?}, Gross PLN: {:.2}, Tax Paid in USD PLN: {:.2}, Cost PLN: {:.2}",
                company,
                gross_pl,
                tax_paid_in_us_pl,
                cost_pl
            );
            companies.push(company.clone());
            gross.push(gross_pl.to_f64().unwrap_or(0.0));
            tax.push(tax_paid_in_us_pl.to_f64().unwrap_or(0.0));
            cost.push(cost_pl.to_f64().unwrap_or(0.0));

            Ok::<(), &str>(())
        })?;
    let series = vec![
        Series::new("Company", companies),
        Series::new("Gross[PLN]", gross),
        Series::new("Cost[PLN]", cost),
        Series::new("Tax Paid in USD[PLN]", tax),
    ];
    DataFrame::new(series)
        .map_err(|_| "Unable to create per company report dataframe")?
        .sort(["Company"], false, true)
        .map_err(|_| "Unable to sort per company report dataframe")
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::Currency;
    use rust_decimal::dec;

    type GainRow = (String, String, Decimal, Decimal, Decimal);
    type SoldRow = (String, String, i32, Decimal, Decimal, Option<String>);
    type GainRowWithQty = (
        String,
        String,
        Decimal,
        Decimal,
        Decimal,
        i32,
        Option<String>,
    );

    fn round4(val: f64) -> f64 {
        (val * 10_000.0).round() / 10_000.0
    }

    fn add_missing_gl_quantity(gains: &[GainRow], sold: &[SoldRow]) -> Vec<GainRowWithQty> {
        let mut sold_qty_by_day: HashMap<chrono::NaiveDate, i32> = HashMap::new();
        let mut sold_symbol_by_day: HashMap<chrono::NaiveDate, Option<String>> = HashMap::new();
        for (trade_date, _settlement_date, qty, _price, _income, _symbol) in sold {
            let day = chrono::NaiveDate::parse_from_str(trade_date, "%m/%d/%y")
                .expect_and_log(&format!("Unable to parse trade date: {trade_date}"));
            *sold_qty_by_day.entry(day).or_insert(0i32) += *qty;

            let candidate_symbol = _symbol.clone();
            sold_symbol_by_day
                .entry(day)
                .and_modify(|existing| {
                    if existing != &candidate_symbol {
                        *existing = None;
                    }
                })
                .or_insert(candidate_symbol);
        }

        let mut gains_by_day: HashMap<chrono::NaiveDate, Vec<usize>> = HashMap::new();
        for (idx, (_acq_date, sold_date, _acq_cost, _cost_basis, _proceeds)) in
            gains.iter().enumerate()
        {
            let day = chrono::NaiveDate::parse_from_str(sold_date, "%m/%d/%Y")
                .expect_and_log(&format!("Unable to parse sold date: {sold_date}"));
            gains_by_day.entry(day).or_insert(vec![]).push(idx);
        }

        let mut out: Vec<GainRowWithQty> = gains
            .iter()
            .map(|(a, b, c, d, e)| (a.clone(), b.clone(), *c, *d, *e, 0i32, None))
            .collect();

        for (day, gain_indices) in gains_by_day {
            let day_sold_qty: i32 = *sold_qty_by_day.get(&day).unwrap_or(&0);
            if day_sold_qty <= 0 {
                continue;
            }
            let day_symbol = sold_symbol_by_day.get(&day).cloned().flatten();

            let day_total_proceeds: Decimal = gain_indices.iter().map(|idx| gains[*idx].4).sum();
            if day_total_proceeds <= Decimal::ZERO {
                continue;
            }

            let mut assigned_sum: i32 = 0;
            for (i, gain_idx) in gain_indices.iter().enumerate() {
                if i + 1 == gain_indices.len() {
                    out[*gain_idx].5 = (day_sold_qty - assigned_sum).max(0);
                    out[*gain_idx].6 = day_symbol.clone();
                } else {
                    let qty = (Decimal::from(day_sold_qty)
                        * (gains[*gain_idx].4 / day_total_proceeds))
                        .round()
                        .to_i32()
                        .unwrap_or(0);
                    out[*gain_idx].5 = qty;
                    out[*gain_idx].6 = day_symbol.clone();
                    assigned_sum += qty;
                }
            }
        }

        out
    }

    #[test]
    fn test_create_per_company_report_interests() -> Result<(), String> {
        let input = vec![
            Transaction {
                transaction_date: "03/01/21".to_string(),
                gross: crate::Currency::EUR(dec!(0.05)),
                tax_paid: crate::Currency::EUR(dec!(0.0)),
                exchange_rate_date: "02/28/21".to_string(),
                exchange_rate: dec!(2.0),
                company: None,
            },
            Transaction {
                transaction_date: "04/11/21".to_string(),
                gross: crate::Currency::EUR(dec!(0.07)),
                tax_paid: crate::Currency::EUR(dec!(0.0)),
                exchange_rate_date: "04/10/21".to_string(),
                exchange_rate: dec!(3.0),
                company: None,
            },
        ];
        let df = create_per_company_report(&input, &[], &[], &[], &[])
            .map_err(|e| format!("Error creating per company report: {}", e))?;

        // Interests are having company == None, and data should be folded to one row
        assert_eq!(df.height(), 1);
        assert_eq!(df.width(), 4);

        let company_col = df.column("Company").unwrap();
        assert_eq!(company_col.get(0).is_err(), false); // None company
        let gross_col = df.column("Gross[PLN]").unwrap();
        assert_eq!(
            round4(gross_col.get(0).unwrap().extract::<f64>().unwrap()),
            round4(0.05 * 2.0 + 0.07 * 3.0)
        );
        let cost_col = df.column("Cost[PLN]").unwrap();
        assert_eq!(cost_col.get(0).unwrap().extract::<f64>().unwrap(), 0.00);
        let tax_col = df.column("Tax Paid in USD[PLN]").unwrap();
        assert_eq!(tax_col.get(0).unwrap().extract::<f64>().unwrap(), 0.00);

        Ok(())
    }

    #[test]
    fn test_create_per_company_report_dividends() -> Result<(), String> {
        let input = vec![
            Transaction {
                transaction_date: "04/11/21".to_string(),
                gross: crate::Currency::USD(dec!(100.0)),
                tax_paid: crate::Currency::USD(dec!(25.0)),
                exchange_rate_date: "04/10/21".to_string(),
                exchange_rate: dec!(3.0),
                company: Some("INTC".to_owned()),
            },
            Transaction {
                transaction_date: "03/01/21".to_string(),
                gross: crate::Currency::USD(dec!(126.0)),
                tax_paid: crate::Currency::USD(dec!(10.0)),
                exchange_rate_date: "02/28/21".to_string(),
                exchange_rate: dec!(2.0),
                company: Some("INTC".to_owned()),
            },
            Transaction {
                transaction_date: "03/11/21".to_string(),
                gross: crate::Currency::USD(dec!(100.0)),
                tax_paid: crate::Currency::USD(dec!(0.0)),
                exchange_rate_date: "02/28/21".to_string(),
                exchange_rate: dec!(10.0),
                company: Some("ABEV".to_owned()),
            },
        ];
        let df = create_per_company_report(&[], &input, &[], &[], &[])
            .map_err(|e| format!("Error creating per company report: {}", e))?;

        // Interests are having company == None, and data should be folded to one row
        assert_eq!(df.height(), 2);
        assert_eq!(df.width(), 4);

        let company_col = df.column("Company").unwrap().utf8().unwrap();
        let gross_col = df.column("Gross[PLN]").unwrap();
        let tax_col = df.column("Tax Paid in USD[PLN]").unwrap();
        let (abev_index, intc_index) = match company_col.get(0) {
            Some("INTC") => (1, 0),
            Some("ABEV") => (0, 1),
            _ => return Err("Unexpected company name in first row".to_owned()),
        };
        assert_eq!(
            round4(gross_col.get(intc_index).unwrap().extract::<f64>().unwrap()),
            round4(100.0 * 3.0 + 126.0 * 2.0)
        );
        assert_eq!(
            round4(gross_col.get(abev_index).unwrap().extract::<f64>().unwrap()),
            round4(100.0 * 10.0)
        );
        assert_eq!(
            tax_col.get(intc_index).unwrap().extract::<f64>().unwrap(),
            round4(25.0 * 3.0 + 10.0 * 2.0)
        );
        assert_eq!(
            tax_col.get(abev_index).unwrap().extract::<f64>().unwrap(),
            round4(0.0)
        );

        let cost_col = df.column("Cost[PLN]").unwrap();
        assert_eq!(cost_col.get(0).unwrap().extract::<f64>().unwrap(), 0.00);
        assert_eq!(cost_col.get(1).unwrap().extract::<f64>().unwrap(), 0.00);

        Ok(())
    }

    #[test]
    fn test_create_per_company_report_sells() -> Result<(), String> {
        let input = vec![
            SoldTransaction {
                trade_date: "03/01/21".to_string(),
                settlement_date: "03/03/21".to_string(),
                acquisition_date: "01/01/21".to_string(),
                income_us: dec!(20.0),
                cost_basis: dec!(20.0),
                fees: dec!(0.0),
                exchange_rate_settlement_date: "02/28/21".to_string(),
                exchange_rate_settlement: dec!(2.5),
                exchange_rate_acquisition_date: "02/28/21".to_string(),
                exchange_rate_acquisition: dec!(5.0),
                company: Some("INTC".to_owned()),
            },
            SoldTransaction {
                trade_date: "06/01/21".to_string(),
                settlement_date: "06/03/21".to_string(),
                acquisition_date: "01/01/19".to_string(),
                income_us: dec!(25.0),
                cost_basis: dec!(10.0),
                fees: dec!(0.0),
                exchange_rate_settlement_date: "05/31/21".to_string(),
                exchange_rate_settlement: dec!(4.0),
                exchange_rate_acquisition_date: "12/30/18".to_string(),
                exchange_rate_acquisition: dec!(6.0),
                company: Some("INTC".to_owned()),
            },
            SoldTransaction {
                trade_date: "06/01/21".to_string(),
                settlement_date: "06/03/21".to_string(),
                acquisition_date: "01/01/19".to_string(),
                income_us: dec!(20.0),
                cost_basis: dec!(0.0),
                fees: dec!(0.0),
                exchange_rate_settlement_date: "05/31/21".to_string(),
                exchange_rate_settlement: dec!(4.0),
                exchange_rate_acquisition_date: "12/30/18".to_string(),
                exchange_rate_acquisition: dec!(6.0),
                company: Some("PXD".to_owned()),
            },
        ];
        let df = create_per_company_report(&[], &[], &input, &[], &[])
            .map_err(|e| format!("Error creating per company report: {}", e))?;

        // Solds are having company
        assert_eq!(df.height(), 2);
        assert_eq!(df.width(), 4);

        let company_col = df.column("Company").unwrap().utf8().unwrap();
        let gross_col = df.column("Gross[PLN]").unwrap();
        let cost_col = df.column("Cost[PLN]").unwrap();
        let (abev_index, intc_index) = match company_col.get(0) {
            Some("INTC") => (1, 0),
            Some("PXD") => (0, 1),
            _ => return Err("Unexpected company name in first row".to_owned()),
        };
        assert_eq!(
            round4(gross_col.get(intc_index).unwrap().extract::<f64>().unwrap()),
            round4(20.0 * 2.5 + 25.0 * 4.0)
        );
        assert_eq!(
            round4(gross_col.get(abev_index).unwrap().extract::<f64>().unwrap()),
            round4(20.0 * 4.0)
        );
        assert_eq!(
            cost_col.get(intc_index).unwrap().extract::<f64>().unwrap(),
            round4(20.0 * 5.0 + 10.0 * 6.0)
        );
        assert_eq!(
            cost_col.get(abev_index).unwrap().extract::<f64>().unwrap(),
            round4(0.0)
        );

        let tax_col = df.column("Tax Paid in USD[PLN]").unwrap();
        assert_eq!(tax_col.get(0).unwrap().extract::<f64>().unwrap(), 0.00);
        assert_eq!(tax_col.get(1).unwrap().extract::<f64>().unwrap(), 0.00);

        Ok(())
    }

    #[test]
    fn test_interests_verification_ok() -> Result<(), String> {
        let transactions: Vec<(String, Decimal, Decimal)> = vec![
            ("06/01/21".to_string(), dec!(100.0), dec!(0.00)),
            ("03/01/21".to_string(), dec!(126.0), dec!(0.00)),
        ];
        verify_interests_transactions(&transactions)
    }

    #[test]
    fn test_revolut_sold_verification_false() -> Result<(), String> {
        let transactions: Vec<(String, String, Currency, Currency, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                "06/01/22".to_string(),
                Currency::PLN(dec!(10.0)),
                Currency::PLN(dec!(2.0)),
                Some("INTC".to_owned()),
            ),
            (
                "06/01/21".to_string(),
                "07/04/23".to_string(),
                Currency::PLN(dec!(10.0)),
                Currency::PLN(dec!(2.0)),
                Some("INTC".to_owned()),
            ),
        ];
        assert_eq!(
            verify_transactions(&transactions),
            Err("Error: Statements are related to different years!".to_owned())
        );
        Ok(())
    }

    #[test]
    fn test_dividends_verification_ok() -> Result<(), String> {
        let transactions: Vec<(String, Decimal, Decimal, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                dec!(100.0),
                dec!(25.0),
                Some("INTC".to_owned()),
            ),
            (
                "03/01/21".to_string(),
                dec!(126.0),
                dec!(10.0),
                Some("INTC".to_owned()),
            ),
        ];
        verify_dividends_transactions(&transactions)
    }

    #[test]
    fn test_dividends_verification_false() -> Result<(), String> {
        let transactions: Vec<(String, Currency, Currency, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                Currency::PLN(dec!(10.0)),
                Currency::PLN(dec!(2.0)),
                Some("INTC".to_owned()),
            ),
            (
                "03/01/22".to_string(),
                Currency::PLN(dec!(126.0)),
                Currency::PLN(dec!(10.0)),
                Some("INTC".to_owned()),
            ),
        ];
        assert_eq!(
            verify_dividends_transactions(&transactions),
            Err("Error:  Statements are related to different years!".to_owned())
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_revolut_transactions_eur() -> Result<(), String> {
        let parsed_transactions = vec![
            (
                "03/01/21".to_owned(),
                crate::Currency::EUR(dec!(0.05)),
                crate::Currency::EUR(dec!(0.00)),
                None,
            ),
            (
                "04/11/21".to_owned(),
                crate::Currency::EUR(dec!(0.07)),
                crate::Currency::EUR(dec!(0.00)),
                None,
            ),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::EUR("03/01/21".to_owned()),
            Some(("02/28/21".to_owned(), dec!(2.0))),
        );
        dates.insert(
            crate::Exchange::EUR("04/11/21".to_owned()),
            Some(("04/10/21".to_owned(), dec!(3.0))),
        );

        let transactions = create_detailed_revolut_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::EUR(dec!(0.05)),
                    tax_paid: crate::Currency::EUR(dec!(0.0)),
                    exchange_rate_date: "02/28/21".to_string(),
                    exchange_rate: dec!(2.0),
                    company: None,
                },
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::EUR(dec!(0.07)),
                    tax_paid: crate::Currency::EUR(dec!(0.0)),
                    exchange_rate_date: "04/10/21".to_string(),
                    exchange_rate: dec!(3.0),
                    company: None,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_revolut_transactions_pln() -> Result<(), String> {
        let parsed_transactions = vec![
            (
                "03/01/21".to_owned(),
                crate::Currency::PLN(dec!(0.44)),
                crate::Currency::PLN(dec!(0.00)),
                None,
            ),
            (
                "04/11/21".to_owned(),
                crate::Currency::PLN(dec!(0.45)),
                crate::Currency::PLN(dec!(0.00)),
                None,
            ),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::PLN("03/01/21".to_owned()),
            Some(("N/A".to_owned(), dec!(1.0))),
        );
        dates.insert(
            crate::Exchange::PLN("04/11/21".to_owned()),
            Some(("N/A".to_owned(), dec!(1.0))),
        );

        let transactions = create_detailed_revolut_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::PLN(dec!(0.44)),
                    tax_paid: crate::Currency::PLN(dec!(0.0)),
                    exchange_rate_date: "N/A".to_string(),
                    exchange_rate: dec!(1.0),
                    company: None,
                },
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::PLN(dec!(0.45)),
                    tax_paid: crate::Currency::PLN(dec!(0.0)),
                    exchange_rate_date: "N/A".to_string(),
                    exchange_rate: dec!(1.0),
                    company: None,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_interests_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, Decimal, Decimal)> = vec![
            ("04/11/21".to_string(), dec!(100.0), dec!(0.00)),
            ("03/01/21".to_string(), dec!(126.0), dec!(0.00)),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::USD("03/01/21".to_owned()),
            Some(("02/28/21".to_owned(), dec!(2.0))),
        );
        dates.insert(
            crate::Exchange::USD("04/11/21".to_owned()),
            Some(("04/10/21".to_owned(), dec!(3.0))),
        );

        let transactions = create_detailed_interests_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::USD(dec!(100.0)),
                    tax_paid: crate::Currency::USD(dec!(0.0)),
                    exchange_rate_date: "04/10/21".to_string(),
                    exchange_rate: dec!(3.0),
                    company: None,
                },
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::USD(dec!(126.0)),
                    tax_paid: crate::Currency::USD(dec!(0.0)),
                    exchange_rate_date: "02/28/21".to_string(),
                    exchange_rate: dec!(2.0),
                    company: None,
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_div_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, Decimal, Decimal, Option<String>)> = vec![
            (
                "04/11/21".to_string(),
                dec!(100.0),
                dec!(25.0),
                Some("INTC".to_owned()),
            ),
            (
                "03/01/21".to_string(),
                dec!(126.0),
                dec!(10.0),
                Some("INTC".to_owned()),
            ),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::USD("03/01/21".to_owned()),
            Some(("02/28/21".to_owned(), dec!(2.0))),
        );
        dates.insert(
            crate::Exchange::USD("04/11/21".to_owned()),
            Some(("04/10/21".to_owned(), dec!(3.0))),
        );

        let transactions = create_detailed_div_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                Transaction {
                    transaction_date: "04/11/21".to_string(),
                    gross: crate::Currency::USD(dec!(100.0)),
                    tax_paid: crate::Currency::USD(dec!(25.0)),
                    exchange_rate_date: "04/10/21".to_string(),
                    exchange_rate: dec!(3.0),
                    company: Some("INTC".to_owned())
                },
                Transaction {
                    transaction_date: "03/01/21".to_string(),
                    gross: crate::Currency::USD(dec!(126.0)),
                    tax_paid: crate::Currency::USD(dec!(10.0)),
                    exchange_rate_date: "02/28/21".to_string(),
                    exchange_rate: dec!(2.0),
                    company: Some("INTC".to_owned())
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_revolut_sold_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(String, String, Currency, Currency, Option<String>)> =
            vec![(
                "11/20/23".to_string(),
                "12/08/24".to_string(),
                Currency::USD(dec!(5000.0)),
                Currency::USD(dec!(5804.62)),
                Some("INTC".to_owned()),
            )];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::USD("11/20/23".to_owned()),
            Some(("11/19/23".to_owned(), dec!(2.0))),
        );
        dates.insert(
            crate::Exchange::USD("12/08/24".to_owned()),
            Some(("12/06/24".to_owned(), dec!(3.0))),
        );

        let transactions = create_detailed_revolut_sold_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![SoldTransaction {
                trade_date: "12/08/24".to_string(),
                settlement_date: "12/08/24".to_string(),
                acquisition_date: "11/20/23".to_string(),
                income_us: dec!(5804.62),
                cost_basis: dec!(5000.0),
                fees: dec!(0.0),
                exchange_rate_settlement_date: "12/06/24".to_string(),
                exchange_rate_settlement: dec!(3.0),
                exchange_rate_acquisition_date: "11/19/23".to_string(),
                exchange_rate_acquisition: dec!(2.0),
                company: Some("INTC".to_owned()),
            },])
        );
        Ok(())
    }

    #[test]
    fn test_create_detailed_sold_transactions() -> Result<(), String> {
        let parsed_transactions: Vec<(
            String,
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                "01/01/21".to_string(),
                dec!(20.0),
                dec!(20.0),
                dec!(0.0), // fees
                Some("INTC".to_owned()),
            ),
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                "01/01/19".to_string(),
                dec!(25.0),
                dec!(10.0),
                dec!(0.0), // fees
                Some("INTC".to_owned()),
            ),
        ];

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>> =
            std::collections::HashMap::new();

        dates.insert(
            crate::Exchange::USD("01/01/21".to_owned()),
            Some(("12/30/20".to_owned(), dec!(1.0))),
        );
        dates.insert(
            crate::Exchange::USD("03/01/21".to_owned()),
            Some(("02/28/21".to_owned(), dec!(2.0))),
        );
        dates.insert(
            crate::Exchange::USD("03/03/21".to_owned()),
            Some(("03/02/21".to_owned(), dec!(2.5))),
        );
        dates.insert(
            crate::Exchange::USD("06/01/21".to_owned()),
            Some(("06/03/21".to_owned(), dec!(3.0))),
        );
        dates.insert(
            crate::Exchange::USD("06/03/21".to_owned()),
            Some(("06/05/21".to_owned(), dec!(4.0))),
        );
        dates.insert(
            crate::Exchange::USD("01/01/21".to_owned()),
            Some(("02/28/21".to_owned(), dec!(5.0))),
        );
        dates.insert(
            crate::Exchange::USD("01/01/19".to_owned()),
            Some(("12/30/18".to_owned(), dec!(6.0))),
        );
        dates.insert(
            crate::Exchange::USD("04/11/21".to_owned()),
            Some(("04/10/21".to_owned(), dec!(7.0))),
        );

        let transactions = create_detailed_sold_transactions(parsed_transactions, &dates);

        assert_eq!(
            transactions,
            Ok(vec![
                SoldTransaction {
                    trade_date: "03/01/21".to_string(),
                    settlement_date: "03/03/21".to_string(),
                    acquisition_date: "01/01/21".to_string(),
                    income_us: dec!(20.0),
                    cost_basis: dec!(20.0),
                    fees: dec!(0.0),
                    exchange_rate_settlement_date: "03/02/21".to_string(),
                    exchange_rate_settlement: dec!(2.5),
                    exchange_rate_acquisition_date: "02/28/21".to_string(),
                    exchange_rate_acquisition: dec!(5.0),
                    company: Some("INTC".to_owned()),
                },
                SoldTransaction {
                    trade_date: "06/01/21".to_string(),
                    settlement_date: "06/03/21".to_string(),
                    acquisition_date: "01/01/19".to_string(),
                    income_us: dec!(25.0),
                    cost_basis: dec!(10.0),
                    fees: dec!(0.0),
                    exchange_rate_settlement_date: "06/05/21".to_string(),
                    exchange_rate_settlement: dec!(4.0),
                    exchange_rate_acquisition_date: "12/30/18".to_string(),
                    exchange_rate_acquisition: dec!(6.0),
                    company: Some("INTC".to_owned()),
                },
            ])
        );
        Ok(())
    }

    #[test]
    fn test_dividends_verification_empty_ok() -> Result<(), String> {
        let transactions: Vec<(String, Decimal, Decimal, Option<String>)> = vec![];
        verify_dividends_transactions(&transactions)
    }

    #[test]
    fn test_dividends_verification_fail() -> Result<(), String> {
        let transactions: Vec<(String, Decimal, Decimal, Option<String>)> = vec![
            (
                "04/11/22".to_string(),
                dec!(100.0),
                dec!(25.0),
                Some("INTC".to_owned()),
            ),
            (
                "03/01/21".to_string(),
                dec!(126.0),
                dec!(10.0),
                Some("INTC".to_owned()),
            ),
        ];
        assert!(verify_dividends_transactions(&transactions).is_err());
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_dividiends_only() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![];
        let empty_trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];

        let (detailed_sold_transactions, _) = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &empty_trade_confirmations,
        )?;
        // 1. trade date
        // 2. settlement date
        // 3. date of purchase
        // 4. net income
        // 5. cost cost basis
        assert_eq!(detailed_sold_transactions, vec![]);
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_ok() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                1,
                dec!(25.0),
                dec!(24.8),
                Some("INTC".to_owned()),
            ),
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                2,
                dec!(10.0),
                dec!(19.8),
                Some("INTC".to_owned()),
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![
            (
                "01/01/2019".to_string(),
                "06/01/2021".to_string(),
                dec!(10.0),
                dec!(10.0),
                dec!(24.8),
            ),
            (
                "01/01/2021".to_string(),
                "03/01/2021".to_string(),
                dec!(20.0),
                dec!(20.0),
                dec!(19.8),
            ),
        ];

        let empty_trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];
        let (detailed_sold_transactions, _) = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &empty_trade_confirmations,
        )?;

        // 1. trade date
        // 2. settlement date
        // 3. date of purchase
        // 4. gross income (or net if no confirmations)
        // 5. cost cost basis
        // 6. fees
        assert_eq!(
            detailed_sold_transactions,
            vec![
                (
                    "06/01/21".to_string(),
                    "06/03/21".to_string(),
                    "01/01/19".to_string(),
                    dec!(24.8),
                    dec!(10.0),
                    dec!(0.0),
                    Some("INTC".to_owned())
                ),
                (
                    "03/01/21".to_string(),
                    "03/03/21".to_string(),
                    "01/01/21".to_string(),
                    dec!(19.8),
                    dec!(20.0),
                    dec!(0.0),
                    Some("INTC".to_owned())
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_single_digits_ok() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> = vec![
            (
                "6/1/21".to_string(),
                "6/3/21".to_string(),
                1,
                dec!(25.0),
                dec!(24.8),
                Some("INTC".to_owned()),
            ),
            (
                "3/1/21".to_string(),
                "3/3/21".to_string(),
                2,
                dec!(10.0),
                dec!(19.8),
                Some("INTC".to_owned()),
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![
            (
                "01/01/2019".to_string(),
                "06/01/2021".to_string(),
                dec!(10.0),
                dec!(10.0),
                dec!(24.8),
            ),
            (
                "01/01/2021".to_string(),
                "03/01/2021".to_string(),
                dec!(20.0),
                dec!(20.0),
                dec!(19.8),
            ),
        ];

        let empty_trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];
        let (detailed_sold_transactions, _) = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &empty_trade_confirmations,
        )?;

        // 1. trade date
        // 2. settlement date
        // 3. date of purchase
        // 4. gross income (or net if no confirmations)
        // 5. cost cost basis
        // 6. fees
        assert_eq!(
            detailed_sold_transactions,
            vec![
                (
                    "06/01/21".to_string(),
                    "6/3/21".to_string(),
                    "01/01/19".to_string(),
                    dec!(24.8),
                    dec!(10.0),
                    dec!(0.0),
                    Some("INTC".to_owned())
                ),
                (
                    "03/01/21".to_string(),
                    "3/3/21".to_string(),
                    "01/01/21".to_string(),
                    dec!(19.8),
                    dec!(20.0),
                    dec!(0.0),
                    Some("INTC".to_owned())
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_second_fail() {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "11/07/22".to_string(),  // trade date
                "11/09/22".to_string(),  // settlement date
                173,                     // quantity
                dec!(28.2035),           // price
                dec!(4877.36),           // amount sold
                Some("INTC".to_owned()), // company symbol (ticker)
            )];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![
            (
                "05/02/22".to_string(), // date when sold stock was acquired (date_acquired)
                "07/19/22".to_string(), // date when stock was sold (date_sold)
                dec!(0.0),              // aqusition cost of sold stock (aquisition_cost)
                dec!(1593.0),           // adjusted aquisition cost of sold stock (cost_basis)
                dec!(1415.480004),      // income from sold stock (total_proceeds)
            ),
            (
                "02/18/22".to_string(),
                "07/19/22".to_string(),
                dec!(4241.16),
                dec!(4989.6),
                dec!(4325.10001),
            ),
            (
                "08/19/22".to_string(),
                "11/07/22".to_string(),
                dec!(5236.0872),
                dec!(6160.0975),
                dec!(4877.355438),
            ),
        ];

        let empty_trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];
        assert_eq!(
            reconstruct_sold_transactions(
                &parsed_sold_transactions,
                &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
                &empty_trade_confirmations
            )
            .is_ok(),
            false
        );
    }

    #[test]
    fn test_sold_transaction_reconstruction_multistock() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> = vec![
            (
                "12/21/22".to_string(),
                "12/23/22".to_string(),
                163,
                dec!(26.5900),
                dec!(4332.44),
                Some("INTC".to_owned()),
            ),
            (
                "12/19/22".to_string(),
                "12/21/22".to_string(),
                252,
                dec!(26.5900),
                dec!(6698.00),
                Some("INTC".to_owned()),
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![
            (
                "08/19/2021".to_string(),
                "12/19/2022".to_string(),
                dec!(4336.4874),
                dec!(4758.6971),
                dec!(2711.0954),
            ),
            (
                "05/03/2021".to_string(),
                "12/21/2022".to_string(),
                dec!(0.0),
                dec!(3876.918),
                dec!(2046.61285),
            ),
            (
                "08/19/2022".to_string(),
                "12/19/2022".to_string(),
                dec!(5045.6257),
                dec!(5936.0274),
                dec!(3986.9048),
            ),
            (
                "05/02/2022".to_string(),
                "12/21/2022".to_string(),
                dec!(0.0),
                dec!(4013.65),
                dec!(2285.82733),
            ),
        ];

        let empty_trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];
        let (detailed_sold_transactions, _) = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &empty_trade_confirmations,
        )?;

        assert_eq!(
            detailed_sold_transactions,
            vec![
                (
                    "12/19/22".to_string(),
                    "12/21/22".to_string(),
                    "08/19/21".to_string(),
                    dec!(2711.0954),
                    dec!(4336.4874),
                    dec!(0.0),
                    Some("INTC".to_owned())
                ),
                (
                    "12/21/22".to_string(),
                    "12/23/22".to_string(),
                    "05/03/21".to_string(),
                    dec!(2046.61285),
                    dec!(0.0),
                    dec!(0.0),
                    Some("INTC".to_owned())
                ),
                (
                    "12/19/22".to_string(),
                    "12/21/22".to_string(),
                    "08/19/22".to_string(),
                    dec!(3986.9048),
                    dec!(5045.6257),
                    dec!(0.0),
                    Some("INTC".to_owned())
                ),
                (
                    "12/21/22".to_string(),
                    "12/23/22".to_string(),
                    "05/02/22".to_string(),
                    dec!(2285.82733),
                    dec!(0.0),
                    dec!(0.0),
                    Some("INTC".to_owned())
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn test_sold_transaction_reconstruction_no_gains_fail() {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> = vec![
            (
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                1,
                dec!(25.0),
                dec!(24.8),
                Some("INTC".to_owned()),
            ),
            (
                "03/01/21".to_string(),
                "03/03/21".to_string(),
                2,
                dec!(10.0),
                dec!(19.8),
                Some("INTC".to_owned()),
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![];

        let empty_trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];
        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &empty_trade_confirmations,
        );
        assert_eq!( result , Err("\n\nERROR: Sold transaction detected, but corressponding Gain&Losses document is missing. Please download Gain&Losses  XLSX document at:\n
            https://us.etrade.com/etx/sp/stockplan#/myAccount/gainsLosses\n\n".to_string()));
    }

    #[test]
    fn test_trade_confirmation_fees_increase_cost_basis_in_detailed_sold() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "06/01/21".to_string(),
                "06/03/21".to_string(),
                1,
                dec!(25.0),
                dec!(24.8),
                Some("INTC".to_owned()),
            )];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![(
            "01/01/2019".to_string(),
            "06/01/2021".to_string(),
            dec!(10.0),
            dec!(10.0),
            dec!(24.8),
        )];

        // net amount should replace income, while commission+fee should be added to cost basis
        let trade_confirmations = vec![(
            "06/01/21".to_string(),
            "06/03/21".to_string(),
            1,
            Decimal::new(2500, 2),
            Decimal::new(2510, 2),
            Decimal::new(20, 2),
            Decimal::new(10, 2),
            Decimal::new(2480, 2),
            Some("INTC".to_owned()),
        )];

        let (reconstructed, _warning) = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &trade_confirmations,
        )?;

        let mut dates: std::collections::HashMap<crate::Exchange, Option<(String, Decimal)>> =
            std::collections::HashMap::new();
        dates.insert(
            crate::Exchange::USD("06/03/21".to_owned()),
            Some(("06/02/21".to_owned(), dec!(4.0))),
        );
        dates.insert(
            crate::Exchange::USD("01/01/19".to_owned()),
            Some(("12/30/18".to_owned(), dec!(3.0))),
        );

        let detailed = create_detailed_sold_transactions(reconstructed, &dates)
            .map_err(|e| format!("Unable to create detailed sold transactions: {e}"))?;

        assert_eq!(detailed.len(), 1);
        assert!((detailed[0].income_us - dec!(24.8)).abs() < dec!(0.0001));
        assert!((detailed[0].cost_basis - dec!(10.3)).abs() < dec!(0.0001));

        Ok(())
    }

    #[test]
    fn test_trade_confirmation_matches_when_date_padding_differs() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "2/12/25".to_string(),
                "2/13/25".to_string(),
                89,
                dec!(10.0),
                dec!(889.0),
                Some("INTC".to_owned()),
            )];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![(
            "01/01/2020".to_string(),
            "02/12/2025".to_string(),
            dec!(500.0),
            dec!(500.0),
            dec!(889.0),
        )];

        let trade_confirmations = vec![(
            "02/12/25".to_string(),
            "02/13/25".to_string(),
            89,
            Decimal::new(1000, 2),
            Decimal::new(8920, 2),
            Decimal::new(20, 2),
            Decimal::new(10, 2),
            Decimal::new(88900, 2),
            Some("INTC".to_owned()),
        )];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &trade_confirmations,
        )?;

        assert_eq!(result.0.len(), 1);
        assert!((result.0[0].3 - dec!(889.0)).abs() < dec!(0.0001));
        assert!((result.0[0].4 - dec!(500.3)).abs() < dec!(0.0001));
        Ok(())
    }

    #[test]
    fn test_trade_confirmation_price_disambiguates_same_day_same_qty_symbol() -> Result<(), String>
    {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> = vec![
            (
                "2/12/25".to_string(),
                "2/13/25".to_string(),
                1,
                dec!(10.00),
                dec!(9.80),
                Some("INTC".to_owned()),
            ),
            (
                "2/12/25".to_string(),
                "2/13/25".to_string(),
                1,
                dec!(11.00),
                dec!(10.75),
                Some("INTC".to_owned()),
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![
            (
                "01/01/2020".to_string(),
                "02/12/2025".to_string(),
                dec!(5.0),
                dec!(5.0),
                dec!(9.80),
            ),
            (
                "01/02/2020".to_string(),
                "02/12/2025".to_string(),
                dec!(6.0),
                dec!(6.0),
                dec!(10.75),
            ),
        ];

        let trade_confirmations = vec![
            (
                "02/12/25".to_string(),
                "02/13/25".to_string(),
                1,
                dec!(11.00),
                dec!(11.00),
                dec!(0.20),
                dec!(0.05),
                dec!(10.75),
                Some("INTC".to_owned()),
            ),
            (
                "02/12/25".to_string(),
                "02/13/25".to_string(),
                1,
                dec!(10.00),
                dec!(10.00),
                dec!(0.15),
                dec!(0.05),
                dec!(9.80),
                Some("INTC".to_owned()),
            ),
        ];

        let (reconstructed, warning) = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &trade_confirmations,
        )?;

        assert_eq!(reconstructed.len(), 2);
        assert_eq!(warning, None);
        assert_eq!(reconstructed[0].3, dec!(9.80));
        assert_eq!(reconstructed[0].4, dec!(5.20));
        assert_eq!(reconstructed[0].5, dec!(0.20));
        assert_eq!(reconstructed[1].3, dec!(10.75));
        assert_eq!(reconstructed[1].4, dec!(6.25));
        assert_eq!(reconstructed[1].5, dec!(0.25));
        Ok(())
    }

    #[test]
    fn test_trade_confirmation_ambiguity_warns_but_proceeds() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> = vec![
            (
                "2/12/25".to_string(),
                "2/13/25".to_string(),
                1,
                dec!(10.00),
                dec!(9.80),
                Some("INTC".to_owned()),
            ),
            (
                "2/12/25".to_string(),
                "2/13/25".to_string(),
                1,
                dec!(10.00),
                dec!(9.70),
                Some("INTC".to_owned()),
            ),
        ];

        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![
            (
                "01/01/2020".to_string(),
                "02/12/2025".to_string(),
                dec!(5.0),
                dec!(5.0),
                dec!(9.80),
            ),
            (
                "01/02/2020".to_string(),
                "02/12/2025".to_string(),
                dec!(6.0),
                dec!(6.0),
                dec!(9.70),
            ),
        ];

        let trade_confirmations = vec![
            (
                "02/12/25".to_string(),
                "02/13/25".to_string(),
                1,
                dec!(10.00),
                dec!(10.00),
                dec!(0.15),
                dec!(0.05),
                dec!(9.80),
                Some("INTC".to_owned()),
            ),
            (
                "02/12/25".to_string(),
                "02/13/25".to_string(),
                1,
                dec!(10.00),
                dec!(10.00),
                dec!(0.20),
                dec!(0.10),
                dec!(9.70),
                Some("INTC".to_owned()),
            ),
        ];

        let (reconstructed, warning) = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &trade_confirmations,
        )?;

        assert_eq!(reconstructed.len(), 2);
        assert!(warning.is_some());
        let warning = warning.unwrap();
        assert!(warning.contains("Ambiguous Trade Confirmation match"));
        assert!(warning.contains("price: 10"));
        Ok(())
    }

    #[test]
    fn test_reconstruction_empty_sold_transactions() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![];
        let parsed_gains_and_losses: Vec<(String, String, Decimal, Decimal, Decimal)> = vec![];
        let trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &add_missing_gl_quantity(&parsed_gains_and_losses, &parsed_sold_transactions),
            &trade_confirmations,
        )?;

        assert_eq!(result.0.len(), 0);
        assert_eq!(result.1, None);
        Ok(())
    }

    #[test]
    fn test_reconstruction_quantity_zero_error() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "1/2/25".to_string(),
                "1/3/25".to_string(),
                100,
                dec!(10.0),
                dec!(1000.0),
                Some("AAPL".to_owned()),
            )];
        let parsed_gains_and_losses: Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )> = vec![(
            "1/1/2020".to_string(),
            "1/2/2025".to_string(),
            dec!(500.0),
            dec!(500.0),
            dec!(1000.0),
            0, // zero quantity should fail
            None,
        )];
        let trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_gains_and_losses,
            &trade_confirmations,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("quantity must be positive"));
        Ok(())
    }

    #[test]
    fn test_reconstruction_quantity_mismatch_error() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "1/2/25".to_string(),
                "1/3/25".to_string(),
                100, // sell 100 shares
                dec!(10.0),
                dec!(1000.0),
                Some("AAPL".to_owned()),
            )];
        let parsed_gains_and_losses: Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )> = vec![(
            "1/1/2020".to_string(),
            "1/2/2025".to_string(),
            dec!(500.0),
            dec!(500.0),
            dec!(1000.0),
            50, // G&L only shows 50 shares - mismatch!
            Some("AAPL".to_owned()),
        )];
        let trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_gains_and_losses,
            &trade_confirmations,
        );

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("Same-day quantity mismatch")
                || err_msg.contains("Unable to allocate")
        );
        assert!(err_msg.contains("1/2/2025") || err_msg.contains("01/02/2025"));
        Ok(())
    }

    #[test]
    fn test_reconstruction_decimal_precision_allocation() -> Result<(), String> {
        // Test that proportional allocation maintains precision with Decimal math
        // Total proceeds: 6172.80 + 6172.80 = 12345.60
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "5/10/25".to_string(),
                "5/12/25".to_string(),
                100,
                dec!(123.456),
                dec!(12345.60), // This matches total G&L proceeds
                Some("GOOGL".to_owned()),
            )];
        let parsed_gains_and_losses: Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )> = vec![
            (
                "3/1/2024".to_string(),
                "5/10/2025".to_string(),
                dec!(6000.0),
                dec!(6000.0),
                dec!(6172.80), // 50% of total
                50,
                Some("GOOGL".to_owned()),
            ),
            (
                "4/1/2024".to_string(),
                "5/10/2025".to_string(),
                dec!(6173.0),
                dec!(6173.0),
                dec!(6172.80), // 50% of total
                50,
                Some("GOOGL".to_owned()),
            ),
        ];

        let trade_confirmations = vec![(
            "05/10/25".to_string(),
            "05/12/25".to_string(),
            100,
            Decimal::new(123456, 3),  // $123.456 per share
            Decimal::new(1234560, 2), // principal
            Decimal::new(2000, 2),    // $20 commission
            Decimal::new(1000, 2),    // $10 fee
            Decimal::new(1234560, 2), // net amount = total proceeds (simplified)
            Some("GOOGL".to_owned()),
        )];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_gains_and_losses,
            &trade_confirmations,
        )?;

        assert_eq!(result.0.len(), 2);
        // Each G&L row should get proportional allocation
        // Income should be 6172.80 (proportional net) for each
        assert_eq!(result.0[0].3, dec!(6172.80));
        assert_eq!(result.0[1].3, dec!(6172.80));

        let total_income: Decimal = result.0.iter().map(|row| row.3).sum();
        let total_fees: Decimal = result.0.iter().map(|row| row.5).sum();
        assert_eq!(total_income, dec!(12345.60));
        assert_eq!(total_fees, dec!(30.0));
        Ok(())
    }

    #[test]
    fn test_reconstruction_repeating_ratio_preserves_exact_totals() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "5/10/25".to_string(),
                "5/12/25".to_string(),
                3,
                dec!(1.0),
                dec!(3.0),
                Some("GOOGL".to_owned()),
            )];

        let parsed_gains_and_losses: Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )> = vec![
            (
                "3/1/2024".to_string(),
                "5/10/2025".to_string(),
                dec!(10.0),
                dec!(10.0),
                dec!(1.0),
                1,
                Some("GOOGL".to_owned()),
            ),
            (
                "4/1/2024".to_string(),
                "5/10/2025".to_string(),
                dec!(11.0),
                dec!(11.0),
                dec!(1.0),
                1,
                Some("GOOGL".to_owned()),
            ),
            (
                "4/2/2024".to_string(),
                "5/10/2025".to_string(),
                dec!(12.0),
                dec!(12.0),
                dec!(1.0),
                1,
                Some("GOOGL".to_owned()),
            ),
        ];

        let trade_confirmations = vec![(
            "05/10/25".to_string(),
            "05/12/25".to_string(),
            3,
            Decimal::new(100, 2),
            Decimal::new(303, 2),
            Decimal::new(2, 2),
            Decimal::new(1, 2),
            Decimal::new(300, 2),
            Some("GOOGL".to_owned()),
        )];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_gains_and_losses,
            &trade_confirmations,
        )?;

        assert_eq!(result.0.len(), 3);

        let total_income: Decimal = result.0.iter().map(|row| row.3).sum();
        let total_fees: Decimal = result.0.iter().map(|row| row.5).sum();
        let total_cost_basis: Decimal = result.0.iter().map(|row| row.4).sum();

        assert_eq!(total_income, dec!(3.0));
        assert_eq!(total_fees, dec!(0.03));
        assert_eq!(total_cost_basis, dec!(33.03));
        assert_ne!(result.0[0].3, result.0[2].3);

        Ok(())
    }

    #[test]
    fn test_reconstruction_uses_quantity_ratio_for_confirmed_split() -> Result<(), String> {
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "2/12/25".to_string(),
                "2/13/25".to_string(),
                89,
                dec!(21.95),
                dec!(1953.49),
                Some("INTC".to_owned()),
            )];

        // Intentionally skew per-lot proceeds away from pure quantity ratio.
        // The fix should allocate confirmed net/fees by quantity, not by these proceeds proportions.
        let parsed_gains_and_losses: Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )> = vec![
            (
                "05/02/2022".to_string(),
                "02/12/2025".to_string(),
                dec!(0.0),
                dec!(0.0),
                dec!(1229.1977574142894852509044149),
                56,
                Some("INTC".to_owned()),
            ),
            (
                "07/15/2022".to_string(),
                "02/12/2025".to_string(),
                dec!(0.0),
                dec!(0.0),
                dec!(724.2922425857105147490955851),
                33,
                Some("INTC".to_owned()),
            ),
        ];

        let trade_confirmations = vec![(
            "02/12/25".to_string(),
            "02/13/25".to_string(),
            89,
            dec!(21.95),
            dec!(1953.55),
            dec!(0.0),
            dec!(0.06),
            dec!(1953.49),
            Some("INTC".to_owned()),
        )];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_gains_and_losses,
            &trade_confirmations,
        )?;

        assert_eq!(result.0.len(), 2);

        // First row should follow 56/89 of confirmed trade totals.
        let first = &result.0[0];
        let first_gross = first.3 + first.5;
        assert_eq!(first.2, "05/02/22".to_string());
        assert!((first_gross - dec!(1229.2)).abs() < dec!(0.0000001));

        // Confirm totals still recompose exactly.
        let total_net: Decimal = result.0.iter().map(|row| row.3).sum();
        let total_fees: Decimal = result.0.iter().map(|row| row.5).sum();
        assert_eq!(total_net, dec!(1953.49));
        assert_eq!(total_fees, dec!(0.06));

        Ok(())
    }

    #[test]
    fn test_reconstruction_multiple_symbols_same_day() -> Result<(), String> {
        // Matching should preserve symbol consistency when both G&L and sold rows contain symbols.
        // Important: PDF total proceeds MUST equal G&L total proceeds
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> = vec![
            (
                "6/1/25".to_string(),
                "6/3/25".to_string(),
                50,
                dec!(100.0),
                dec!(5000.0), // AAPL: 50 shares * $100
                Some("AAPL".to_owned()),
            ),
            (
                "6/1/25".to_string(),
                "6/3/25".to_string(),
                50,
                dec!(100.0),  // same price
                dec!(5000.0), // MSFT: 50 shares * $100
                Some("MSFT".to_owned()),
            ),
        ];
        // Total PDF proceeds: 5000 + 5000 = 10000
        let parsed_gains_and_losses: Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )> = vec![
            (
                "1/1/2024".to_string(),
                "6/1/2025".to_string(),
                dec!(1000.0),
                dec!(1000.0),
                dec!(3000.0), // AAPL first lot
                25,
                Some("AAPL".to_owned()),
            ),
            (
                "2/1/2024".to_string(),
                "6/1/2025".to_string(),
                dec!(2000.0),
                dec!(2000.0),
                dec!(2000.0), // AAPL second lot
                25,
                Some("AAPL".to_owned()),
            ),
            (
                "3/1/2024".to_string(),
                "6/1/2025".to_string(),
                dec!(5000.0),
                dec!(5000.0),
                dec!(5000.0), // MSFT
                50,
                Some("MSFT".to_owned()),
            ),
        ];
        // Total G&L proceeds: 3000 + 2000 + 5000 = 10000 ✓ matches PDF

        let trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_gains_and_losses,
            &trade_confirmations,
        )?;

        assert_eq!(result.0.len(), 3);
        let aapl_total: Decimal = result
            .0
            .iter()
            .filter(|r| r.6.as_deref() == Some("AAPL"))
            .map(|r| r.3)
            .sum();
        let msft_total: Decimal = result
            .0
            .iter()
            .filter(|r| r.6.as_deref() == Some("MSFT"))
            .map(|r| r.3)
            .sum();

        assert_eq!(aapl_total, dec!(5000.0));
        assert_eq!(msft_total, dec!(5000.0));
        let total_proceeds: Decimal = result.0.iter().map(|r| r.3).sum();
        assert_eq!(total_proceeds, dec!(10000.0));
        Ok(())
    }

    #[test]
    fn test_reconstruction_allocation_cannot_be_satisfied() -> Result<(), String> {
        // Test infeasible knapsack: bought 10, trying to sell 100
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "7/5/25".to_string(),
                "7/7/25".to_string(),
                100, // trying to sell 100
                dec!(50.0),
                dec!(5000.0),
                Some("TSLA".to_owned()),
            )];
        let parsed_gains_and_losses: Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )> = vec![(
            "1/15/2024".to_string(),
            "7/5/2025".to_string(),
            dec!(10.0),
            dec!(10.0),
            dec!(5000.0),
            10, // only 10 shares bought - impossible to sell 100
            Some("TSLA".to_owned()),
        )];
        let trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_gains_and_losses,
            &trade_confirmations,
        );

        assert!(result.is_err());
        let err_msg = result.unwrap_err();
        // Either quantity mismatch or unable to allocate
        assert!(
            err_msg.contains("mismatch") || err_msg.contains("Unable to allocate"),
            "Expected error message to mention mismatch or allocation failure, got: {err_msg}"
        );
        Ok(())
    }

    #[test]
    fn test_reconstruction_date_normalization_consistency() -> Result<(), String> {
        // Ensure consistent date handling across different padding styles
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> = vec![
            (
                "8/9/25".to_string(),
                "8/10/25".to_string(),
                30,
                dec!(75.0),
                dec!(2250.0),
                Some("IBM".to_owned()),
            ),
            (
                "08/09/25".to_string(),
                "8/11/25".to_string(),
                20,
                dec!(75.0),
                dec!(1500.0),
                Some("IBM".to_owned()),
            ),
        ];
        let parsed_gains_and_losses: Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )> = vec![
            (
                "10/1/2023".to_string(),
                "8/9/2025".to_string(),
                dec!(100.0),
                dec!(100.0),
                dec!(2250.0),
                30,
                Some("IBM".to_owned()),
            ),
            (
                "11/1/2023".to_string(),
                "8/9/2025".to_string(),
                dec!(200.0),
                dec!(200.0),
                dec!(1500.0),
                20,
                Some("IBM".to_owned()),
            ),
        ];
        let trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_gains_and_losses,
            &trade_confirmations,
        )?;

        assert_eq!(result.0.len(), 2);
        // Both should match despite different padding in inputs
        assert!((result.0[0].3 - dec!(2250.0)).abs() < dec!(0.0001));
        assert!((result.0[1].3 - dec!(1500.0)).abs() < dec!(0.0001));
        Ok(())
    }

    #[test]
    fn test_reconstruction_missing_trade_confirmation_warning() -> Result<(), String> {
        // Verify warning is returned when no trade confirmations provided
        let parsed_sold_transactions: Vec<(String, String, i32, Decimal, Decimal, Option<String>)> =
            vec![(
                "9/1/25".to_string(),
                "9/3/25".to_string(),
                25,
                dec!(150.0),
                dec!(3750.0),
                Some("META".to_owned()),
            )];
        let parsed_gains_and_losses: Vec<(
            String,
            String,
            Decimal,
            Decimal,
            Decimal,
            i32,
            Option<String>,
        )> = vec![(
            "4/1/2024".to_string(),
            "9/1/2025".to_string(),
            dec!(800.0),
            dec!(800.0),
            dec!(3750.0),
            25,
            Some("META".to_owned()),
        )];
        let trade_confirmations: Vec<(
            String,
            String,
            i32,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Decimal,
            Option<String>,
        )> = vec![];

        let result = reconstruct_sold_transactions(
            &parsed_sold_transactions,
            &parsed_gains_and_losses,
            &trade_confirmations,
        )?;

        assert_eq!(result.0.len(), 1);
        assert!(result.1.is_some());
        let warning_msg = result.1.unwrap();
        assert!(warning_msg.contains("Trade Confirmation"));
        assert!(warning_msg.contains("Fees"));
        Ok(())
    }
}
