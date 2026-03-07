use chrono::{DateTime, Utc};

use application::services::outlook_client::OutlookEvent;

use super::types::GraphEvent;

/// Map a Microsoft Graph calendar event to the application-layer OutlookEvent DTO.
pub fn map_graph_event(event: GraphEvent) -> Option<OutlookEvent> {
    let start_time: DateTime<Utc> = event.start.date_time.parse().ok()?;
    let end_time: DateTime<Utc> = event.end.date_time.parse().ok()?;

    Some(OutlookEvent {
        outlook_id: event.id,
        title: event.subject,
        start_time,
        end_time,
        location: event.location.and_then(|l| l.display_name),
        participants: event
            .attendees
            .into_iter()
            .map(|a| a.email_address.name)
            .collect(),
        is_cancelled: event.is_cancelled,
    })
}
