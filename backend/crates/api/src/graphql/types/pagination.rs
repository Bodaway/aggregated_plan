use async_graphql::SimpleObject;

/// Relay-style pagination info.
#[derive(SimpleObject, Debug, Clone)]
pub struct PageInfo {
    /// Whether there are more items after the last edge.
    pub has_next_page: bool,
    /// Whether there are items before the first edge.
    pub has_previous_page: bool,
    /// Cursor of the last edge in the current page.
    pub end_cursor: Option<String>,
    /// Cursor of the first edge in the current page.
    pub start_cursor: Option<String>,
}

impl PageInfo {
    /// Create a PageInfo indicating no more pages and no cursors.
    pub fn empty() -> Self {
        PageInfo {
            has_next_page: false,
            has_previous_page: false,
            end_cursor: None,
            start_cursor: None,
        }
    }
}
