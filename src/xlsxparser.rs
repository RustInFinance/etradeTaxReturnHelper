use calamine::{open_workbook, Reader, Xlsx};

pub use crate::logging::ResultExt;

/// This function parses G&L Collappsed and Expanded for needed transaction details
pub fn parse_gains_and_losses(xlsxtoparse: &str) -> Vec<(String, String, f32, f32, f32)> {
    let mut excel: Xlsx<_> = open_workbook(xlsxtoparse)
        .expect_and_log(&format!("Error opening XLSX file: {}", xlsxtoparse));
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
                    "Date Acquired" => date_acquired_idx = idx,
                    "Date Sold" => date_sold_idx = idx,
                    "Acquisition Cost" => acquistion_cost_idx = idx,
                    "Adjusted Cost Basis" => cost_basis_idx = idx,
                    "Total Proceeds" => total_proceeds_idx = idx,
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
    transactions
}

#[test]
fn test_parse_gain_and_losses() -> Result<(), String> {
    assert_eq!(
        parse_gains_and_losses("data/G&L_Collapsed.xlsx"),
        (vec![
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
    assert_eq!(
        parse_gains_and_losses("data/G&L_Expanded.xlsx"),
        (vec![
            (
                "04/24/2013".to_owned(),
                "04/11/2022".to_owned(),
                0.0,
                23.5175,
                46.9
            ),
        ])
    );

    Ok(())
}
