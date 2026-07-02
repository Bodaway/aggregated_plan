use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, TimeZone, Utc};
use chrono_tz::Tz;

pub const DEFAULT_TZ: &str = "Europe/Paris";

/// Resolve a timezone from a config string, falling back to Europe/Paris.
pub fn resolve_tz(config_value: Option<String>) -> Tz {
    config_value
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| DEFAULT_TZ.parse().expect("default tz parses"))
}

/// Convert a UTC instant to the user's local wall-clock (naive) time.
pub fn to_local(dt: DateTime<Utc>, tz: Tz) -> NaiveDateTime {
    tz.from_utc_datetime(&dt.naive_utc()).naive_local()
}

/// UTC bounds [start, end) for a LOCAL calendar day. `end` is the next local midnight.
/// Uses the earliest valid instant at each local midnight (handles DST gaps).
pub fn local_day_bounds(date: NaiveDate, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_local = date.and_hms_opt(0, 0, 0).expect("valid midnight");
    let end_local = (date + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("valid midnight");
    let start_utc = tz
        .from_local_datetime(&start_local)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&start_local));
    let end_utc = tz
        .from_local_datetime(&end_local)
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&end_local));
    (start_utc, end_utc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_tz_defaults_to_paris() {
        assert_eq!(resolve_tz(None), "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(resolve_tz(Some("bogus".into())), "Europe/Paris".parse::<Tz>().unwrap());
        assert_eq!(resolve_tz(Some("UTC".into())), Tz::UTC);
    }

    #[test]
    fn paris_local_day_is_offset_from_utc() {
        // 2026-06-08 is CEST (UTC+2): local midnight = 22:00 UTC the previous day.
        let tz: Tz = "Europe/Paris".parse().unwrap();
        let (start, end) = local_day_bounds(NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(), tz);
        assert_eq!(start.to_rfc3339(), "2026-06-07T22:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-06-08T22:00:00+00:00");
    }

    #[test]
    fn to_local_shifts_into_paris() {
        let tz: Tz = "Europe/Paris".parse().unwrap();
        let utc = DateTime::parse_from_rfc3339("2026-06-08T07:30:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let local = to_local(utc, tz);
        assert_eq!(local.to_string(), "2026-06-08 09:30:00"); // +2h CEST
    }
}
