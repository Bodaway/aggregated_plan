use chrono::{DateTime, Duration, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
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

/// The instant a local day begins, in UTC.
///
/// A local midnight that does not exist — a zone whose DST jump lands on 00:00,
/// as Cuba's does — is walked forward to the first instant that does, rather than
/// reinterpreted as UTC outright: that reinterpretation is off by the zone's whole
/// offset, and used to be exactly how `local_day_bounds` and this function (then
/// `reattribution::local_day_start`) disagreed on the day a backdated entry fell
/// in. There is now exactly one implementation of "which UTC instant starts a
/// local day", which both the flush and the reattribution repair read through
/// [`local_window`].
pub fn local_day_start(tz: Tz, date: NaiveDate) -> DateTime<Utc> {
    let midnight = date.and_time(NaiveTime::MIN);
    if let Some(local) = tz.from_local_datetime(&midnight).earliest() {
        return local.with_timezone(&Utc);
    }
    tz.from_local_datetime(&(midnight + Duration::hours(1)))
        .earliest()
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|| Utc.from_utc_datetime(&midnight))
}

/// UTC half-open window `[start of `since`, start of the day after `until`)`,
/// matching the repository's `logged_at >= from AND logged_at < to`.
///
/// The one local-day-to-UTC conversion the flush and the reattribution repair
/// share: two implementations of "which UTC instants a local day spans" could
/// disagree, and a disagreement there puts one entry on two different local days.
pub fn local_window(tz: Tz, since: NaiveDate, until: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    (
        local_day_start(tz, since),
        local_day_start(tz, until.succ_opt().unwrap_or(until)),
    )
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
        let (start, end) = local_window(tz, NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(), NaiveDate::from_ymd_opt(2026, 6, 8).unwrap());
        assert_eq!(start.to_rfc3339(), "2026-06-07T22:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-06-08T22:00:00+00:00");
    }

    /// The hazard this consolidation exists to end: on a local midnight that does
    /// not exist — Cuba's 2026 DST jump lands at 00:00, clocks springing forward
    /// straight to 01:00 — a naive fallback that reinterprets the midnight as UTC
    /// outright lands five hours (the zone's whole standard offset) before the
    /// instant that actually starts the day. `local_day_start` walks forward to the
    /// first instant that exists instead: 01:00 local, already at the DST offset,
    /// which is 05:00 UTC. Deliberately not Europe/Paris — its DST jump never lands
    /// on a local midnight, so this bug is invisible there.
    #[test]
    fn havana_local_window_walks_forward_past_the_dst_gap() {
        let tz: Tz = "America/Havana".parse().unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        let (start, end) = local_window(tz, date, date);
        assert_eq!(
            start.to_rfc3339(),
            "2026-03-08T05:00:00+00:00",
            "must walk forward to 01:00 local (already DST), not reinterpret 00:00 as UTC"
        );
        assert_eq!(end.to_rfc3339(), "2026-03-09T04:00:00+00:00");
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
