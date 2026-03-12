use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};

use application::services::outlook_client::OutlookEvent;

use super::types::GraphEvent;

/// Parse a Microsoft Graph datetime string to `DateTime<Utc>`.
/// Graph returns datetimes without timezone suffix (e.g. "2026-03-12T10:00:00.0000000")
/// with a separate `timeZone` field. We treat the value as UTC (Graph sets timeZone="UTC"
/// when the calendar timezone is UTC; for other timezones the offset is not embedded).
fn parse_graph_dt(s: &str) -> Option<DateTime<Utc>> {
    // Try RFC3339 first (has timezone suffix).
    if let Ok(dt) = s.parse::<DateTime<Utc>>() {
        return Some(dt);
    }
    // Fall back: naive datetime without timezone (Graph format with fractional seconds).
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f").ok()?;
    Some(Utc.from_utc_datetime(&naive))
}

/// Map a Microsoft Graph calendar event to the application-layer OutlookEvent DTO.
pub fn map_graph_event(event: GraphEvent) -> Option<OutlookEvent> {
    let start_time = parse_graph_dt(&event.start.date_time)?;
    let end_time = parse_graph_dt(&event.end.date_time)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::*;

    fn make_event(
        id: &str,
        subject: &str,
        start: &str,
        end: &str,
        is_cancelled: bool,
        location_name: Option<&str>,
        attendees: Vec<(&str, &str)>,
    ) -> GraphEvent {
        GraphEvent {
            id: id.to_string(),
            subject: subject.to_string(),
            start: GraphDateTime {
                date_time: start.to_string(),
                time_zone: "UTC".to_string(),
            },
            end: GraphDateTime {
                date_time: end.to_string(),
                time_zone: "UTC".to_string(),
            },
            location: location_name.map(|n| GraphLocation {
                display_name: Some(n.to_string()),
            }),
            attendees: attendees
                .into_iter()
                .map(|(name, addr)| GraphAttendee {
                    email_address: GraphEmailAddress {
                        name: name.to_string(),
                        address: addr.to_string(),
                    },
                })
                .collect(),
            is_cancelled,
        }
    }

    #[test]
    fn maps_valid_event() {
        let event = make_event(
            "evt-1",
            "Team Standup",
            "2026-03-10T09:00:00+00:00",
            "2026-03-10T09:30:00+00:00",
            false,
            Some("Room 42"),
            vec![("Alice", "alice@example.com"), ("Bob", "bob@example.com")],
        );

        let result = map_graph_event(event).unwrap();

        assert_eq!(result.outlook_id, "evt-1");
        assert_eq!(result.title, "Team Standup");
        assert_eq!(result.location, Some("Room 42".to_string()));
        assert_eq!(result.participants.len(), 2);
        assert_eq!(result.participants[0], "Alice");
        assert_eq!(result.participants[1], "Bob");
        assert!(!result.is_cancelled);
    }

    #[test]
    fn maps_event_without_location() {
        let event = make_event(
            "evt-2",
            "Quick Chat",
            "2026-03-10T10:00:00+00:00",
            "2026-03-10T10:15:00+00:00",
            false,
            None,
            vec![],
        );

        let result = map_graph_event(event).unwrap();

        assert!(result.location.is_none());
        assert!(result.participants.is_empty());
    }

    #[test]
    fn returns_none_for_invalid_start_time() {
        let event = make_event(
            "evt-3",
            "Bad Event",
            "not-a-date",
            "2026-03-10T10:00:00+00:00",
            false,
            None,
            vec![],
        );

        assert!(map_graph_event(event).is_none());
    }

    #[test]
    fn returns_none_for_invalid_end_time() {
        let event = make_event(
            "evt-4",
            "Bad Event",
            "2026-03-10T10:00:00+00:00",
            "bad-end",
            false,
            None,
            vec![],
        );

        assert!(map_graph_event(event).is_none());
    }

    #[test]
    fn cancelled_event_still_maps() {
        // The mapper does not filter cancelled events -- that is the client's job.
        let event = make_event(
            "evt-5",
            "Cancelled Meeting",
            "2026-03-10T09:00:00+00:00",
            "2026-03-10T10:00:00+00:00",
            true,
            None,
            vec![],
        );

        let result = map_graph_event(event).unwrap();
        assert!(result.is_cancelled);
    }
}
