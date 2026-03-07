use std::collections::HashMap;

use application::services::excel_client::{ExcelMappingConfig, ExcelRow};

use super::types::GraphWorksheetRange;

/// Map a Graph API worksheet range to application-layer ExcelRow DTOs.
/// The first row is treated as the header row.
pub fn map_worksheet_range(
    range: GraphWorksheetRange,
    _config: &ExcelMappingConfig,
) -> Vec<ExcelRow> {
    let mut rows = range.values.into_iter();

    let headers: Vec<String> = match rows.next() {
        Some(header_row) => header_row
            .into_iter()
            .map(|v| v.as_str().unwrap_or("").to_string())
            .collect(),
        None => return Vec::new(),
    };

    rows.enumerate()
        .map(|(idx, row)| {
            let columns: HashMap<String, String> = headers
                .iter()
                .zip(row.into_iter())
                .map(|(header, cell)| {
                    let value = match cell {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    };
                    (header.clone(), value)
                })
                .collect();

            ExcelRow {
                row_index: idx + 1,
                columns,
            }
        })
        .collect()
}
