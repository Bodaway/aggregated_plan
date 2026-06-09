use serde::Deserialize;

/// Microsoft Graph API calendar response types.

#[derive(Debug, Deserialize)]
pub struct GraphCalendarResponse {
    pub value: Vec<GraphEvent>,
    #[serde(rename = "@odata.nextLink")]
    pub odata_next_link: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEvent {
    pub id: String,
    pub subject: String,
    pub start: GraphDateTime,
    pub end: GraphDateTime,
    pub location: Option<GraphLocation>,
    pub attendees: Vec<GraphAttendee>,
    pub is_cancelled: bool,
    pub show_as: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphDateTime {
    pub date_time: String,
    pub time_zone: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphLocation {
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphAttendee {
    pub email_address: GraphEmailAddress,
}

#[derive(Debug, Deserialize)]
pub struct GraphEmailAddress {
    pub name: String,
    pub address: String,
}
