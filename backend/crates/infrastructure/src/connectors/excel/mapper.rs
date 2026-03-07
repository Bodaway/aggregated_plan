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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn default_config() -> ExcelMappingConfig {
        ExcelMappingConfig {
            sharepoint_path: "test.xlsx".to_string(),
            sheet_name: None,
            title_column: "Title".to_string(),
            assignee_column: Some("Assignee".to_string()),
            project_column: None,
            date_column: None,
            jira_key_column: None,
            status_column: None,
        }
    }

    #[test]
    fn maps_worksheet_with_header_and_data_rows() {
        let range = GraphWorksheetRange {
            values: vec![
                vec![json!("Title"), json!("Assignee"), json!("Status")],
                vec![json!("Task A"), json!("Alice"), json!("Done")],
                vec![json!("Task B"), json!("Bob"), json!("To Do")],
            ],
        };

        let rows = map_worksheet_range(range, &default_config());

        assert_eq!(rows.len(), 2);

        assert_eq!(rows[0].row_index, 1);
        assert_eq!(rows[0].columns.get("Title").unwrap(), "Task A");
        assert_eq!(rows[0].columns.get("Assignee").unwrap(), "Alice");
        assert_eq!(rows[0].columns.get("Status").unwrap(), "Done");

        assert_eq!(rows[1].row_index, 2);
        assert_eq!(rows[1].columns.get("Title").unwrap(), "Task B");
    }

    #[test]
    fn returns_empty_for_empty_range() {
        let range = GraphWorksheetRange { values: vec![] };
        let rows = map_worksheet_range(range, &default_config());
        assert!(rows.is_empty());
    }

    #[test]
    fn returns_empty_for_header_only() {
        let range = GraphWorksheetRange {
            values: vec![vec![json!("Title"), json!("Assignee")]],
        };
        let rows = map_worksheet_range(range, &default_config());
        assert!(rows.is_empty());
    }

    #[test]
    fn handles_numeric_and_null_values() {
        let range = GraphWorksheetRange {
            values: vec![
                vec![json!("Name"), json!("Count"), json!("Empty")],
                vec![json!("Item"), json!(42), json!(null)],
            ],
        };

        let rows = map_worksheet_range(range, &default_config());

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].columns.get("Name").unwrap(), "Item");
        assert_eq!(rows[0].columns.get("Count").unwrap(), "42");
        assert_eq!(rows[0].columns.get("Empty").unwrap(), "null");
    }

    #[test]
    fn handles_fewer_data_columns_than_headers() {
        let range = GraphWorksheetRange {
            values: vec![
                vec![json!("A"), json!("B"), json!("C")],
                vec![json!("only-one")],
            ],
        };

        let rows = map_worksheet_range(range, &default_config());

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].columns.len(), 1);
        assert_eq!(rows[0].columns.get("A").unwrap(), "only-one");
    }
}
