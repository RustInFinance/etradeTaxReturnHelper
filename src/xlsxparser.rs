use calamine::{open_workbook, Reader, Xlsx};

pub use crate::logging::ResultExt;

/// This function parses G&L Collappsed and Expanded for needed transaction details
/// and it returns found sold transactions in a form:
/// date when sold stock was acquired (date_acquired)
/// date when stock was sold (date_sold)
/// aqusition cost of sold stock (aquisition_cost)
/// adjusted aquisition cost of sold stock (cost_basis)
/// income from sold stock (total_proceeds)
pub fn parse_gains_and_losses(
    xlsxtoparse: &str,
) -> Result<Vec<(String, String, f32, f32, f32)>, &str> {
    let mut excel: Xlsx<_> =
        open_workbook(xlsxtoparse).map_err(|_| "Error opening XLSX file: {}")?;
    let name = excel
        .sheet_names()
        .first()
        .expect_and_log("No worksheet found")
        .clone();
    log::info!("name: {}", name);
    let mut transactions: Vec<(String, String, f32, f32, f32)> = vec![];
    if let Some(Ok(r)) = excel.worksheet_range(&name) {
        let mut rows = r.rows();
        let categories = rows
            .next()
            .expect_and_log("Error: unable to get descriptive row");
        let mut date_acquired_idx = 0;
        let mut date_sold_idx = 0;
        let mut cost_basis_idx = 0;
        let mut acquistion_cost_idx = 0;
        let mut total_proceeds_idx = 0;

        let mut idx = 0;
        for c in categories {
            // Find indices of interesting collumns
            if let Some(v) = c.get_string() {
                match v {
                    "Date Acquired" | "Data nabycia" => date_acquired_idx = idx,
                    "Date Sold" | "Data sprzedaży" => date_sold_idx = idx,
                    "Acquisition Cost" | "Koszt zakupu" => acquistion_cost_idx = idx,
                    "Adjusted Cost Basis" | "Skorygowana podstawa kosztów" => cost_basis_idx = idx,
                    "Total Proceeds" | "Łączne wpływy" => total_proceeds_idx = idx,
                    _ => (),
                }
            }

            idx = idx + 1;
        }

        // Rewind summary row as we are not interested in this
        rows.next();

        // Iterate through rows of actual sold transactions
        for transakcja in rows {
            log::info!(
                "G&L ACQUIRED_DATE: {} SOLD_DATE: {} ACQUISTION_COST: {} COST_BASIS: {} TOTAL: {}",
                transakcja[date_acquired_idx],
                transakcja[date_sold_idx],
                transakcja[acquistion_cost_idx],
                transakcja[cost_basis_idx],
                transakcja[total_proceeds_idx]
            );
            // If row is ill formed or emtpy then it means user added something and this is to be
            // dropped
            if transakcja[date_acquired_idx].is_empty()
                && transakcja[date_sold_idx].is_empty()
                && transakcja[acquistion_cost_idx].is_empty()
                && transakcja[cost_basis_idx].is_empty()
                && transakcja[total_proceeds_idx].is_empty()
            {
                log::info!(
                    "G&L Finished parsing due to empty raw of data. Did you modified document?"
                );
                break;
            }

            //println!("transakcja: {:?}", transakcja);
            transactions.push((
                transakcja[date_acquired_idx]
                    .get_string()
                    .unwrap()
                    .to_owned(),
                transakcja[date_sold_idx].get_string().unwrap().to_owned(),
                transakcja[acquistion_cost_idx].get_float().unwrap() as f32,
                transakcja[cost_basis_idx].get_float().unwrap() as f32,
                transakcja[total_proceeds_idx].get_float().unwrap() as f32,
            ));
        }
    }
    log::info!("G&L Transactions: {:#?}", transactions);
    Ok(transactions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gain_and_losses() -> Result<(), String> {
        assert_eq!(
            parse_gains_and_losses("data/G&L_Collapsed.xlsx"),
            Ok((vec![
                (
                    "04/24/2013".to_owned(),
                    "04/11/2022".to_owned(),
                    0.0,
                    23.5175,
                    46.9
                ),
                (
                    "08/19/2015".to_owned(),
                    "05/02/2022".to_owned(),
                    24.258,
                    29.28195,
                    43.67
                )
            ]))
        );
        assert_eq!(
            parse_gains_and_losses("data/G&L_Expanded.xlsx"),
            Ok(vec![
                (
                    "04/24/2013".to_owned(),
                    "04/11/2022".to_owned(),
                    0.0,
                    23.5175,
                    46.9
                ),
                (
                    "08/19/2015".to_owned(),
                    "05/02/2022".to_owned(),
                    24.258,
                    29.28195,
                    43.67
                )
            ])
        );

        Ok(())
    }

    #[test]
    fn test_parse_gain_and_losses_pl() -> Result<(), String> {
        assert_eq!(
            parse_gains_and_losses("data/G&L_Expanded_polish.xlsx"),
            Ok(vec![
                (
                    "02/17/2023".to_owned(),
                    "02/21/2023".to_owned(),
                    1791.0388,
                    2107.1,
                    2018.3545
                ),
                (
                    "08/01/2022".to_owned(),
                    "06/05/2023".to_owned(),
                    0.0,
                    258.09,
                    219.0275
                ),
                (
                    "01/31/2023".to_owned(),
                    "06/05/2023".to_owned(),
                    0.0,
                    195.37,
                    219.0275
                ),
                (
                    "10/31/2022".to_owned(),
                    "06/05/2023".to_owned(),
                    0.0,
                    200.305,
                    219.0275
                ),
                (
                    "05/01/2023".to_owned(),
                    "06/05/2023".to_owned(),
                    0.0,
                    215.32,
                    219.0275
                ),
                (
                    "07/31/2023".to_owned(),
                    "08/07/2023".to_owned(),
                    0.0,
                    255.0275,
                    247.16
                ),
                (
                    "08/18/2023".to_owned(),
                    "08/21/2023".to_owned(),
                    1969.0505,
                    2701.235,
                    2689.0754
                ),
                (
                    "08/30/2023".to_owned(),
                    "12/13/2023".to_owned(),
                    0.0,
                    923.8725,
                    1187.31
                ),
                (
                    "11/30/2023".to_owned(),
                    "12/13/2023".to_owned(),
                    0.0,
                    1163.5,
                    1143.34
                ),
                (
                    "10/31/2023".to_owned(),
                    "12/13/2023".to_owned(),
                    0.0,
                    252.665,
                    307.82
                )
            ])
        );
        Ok(())
    }
}
