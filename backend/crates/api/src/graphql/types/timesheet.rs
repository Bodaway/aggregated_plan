use async_graphql::{InputObject, SimpleObject, ID};
use chrono::{NaiveDate, NaiveDateTime};

use domain::rules::reconstruction::{
    AttributedBlock, EditedLine, ProjectAllocation, ReconstructedDay, UnresolvedSignal,
};
use domain::types::{Confidence, SignalMapping, TimesheetDraft, TimesheetDraftLine, TimesheetStatus};

use super::enums::{BlockKindGql, ConfidenceGql, MappingKindGql, TimesheetStatusGql};

#[derive(SimpleObject)]
pub struct TimesheetLineGql {
    pub gryzzly_project_id: Option<String>,
    pub project_name: Option<String>,
    pub hours: f64,
    pub is_pinned: bool,
    pub confidence: ConfidenceGql,
    pub source_refs: Vec<String>,
}

#[derive(SimpleObject)]
pub struct AttributedBlockGql {
    pub start_time: NaiveDateTime,
    pub end_time: NaiveDateTime,
    pub gryzzly_project_id: Option<String>,
    pub kind: BlockKindGql,
    pub hours: f64,
    pub source_refs: Vec<String>,
    /// Secondary display label: the name of what the block came from — the owning task's
    /// title for a WORK block, the meeting subject for a MEETING block. Null when the
    /// origin has no known name, or on a day persisted before the field existed.
    pub origin_label: Option<String>,
}

impl From<AttributedBlock> for AttributedBlockGql {
    fn from(b: AttributedBlock) -> Self {
        Self {
            start_time: b.start,
            end_time: b.end,
            gryzzly_project_id: b.gryzzly_project_id,
            kind: b.kind.into(),
            hours: b.hours,
            source_refs: b.source_refs,
            origin_label: b.origin_label,
        }
    }
}

#[derive(SimpleObject)]
pub struct UnresolvedSignalGql {
    pub source_ref: String,
    pub label: String,
    pub at: NaiveDateTime,
}

impl From<UnresolvedSignal> for UnresolvedSignalGql {
    fn from(u: UnresolvedSignal) -> Self {
        Self { source_ref: u.source_ref, label: u.label, at: u.at }
    }
}

#[derive(SimpleObject)]
pub struct SignalMappingGql {
    pub id: ID,
    pub kind: MappingKindGql,
    pub pattern: String,
    pub branch_pattern: Option<String>,
    pub gryzzly_project_id: String,
    pub gryzzly_project_name: Option<String>,
    pub is_enabled: bool,
}

impl From<SignalMapping> for SignalMappingGql {
    fn from(m: SignalMapping) -> Self {
        Self {
            id: ID(m.id.to_string()),
            kind: m.kind.into(),
            pattern: m.pattern,
            branch_pattern: m.branch_pattern,
            gryzzly_project_id: m.gryzzly_project_id,
            gryzzly_project_name: m.gryzzly_project_name,
            is_enabled: m.is_enabled,
        }
    }
}

#[derive(SimpleObject)]
pub struct ReconstructedDayGql {
    pub date: NaiveDate,
    pub status: TimesheetStatusGql,
    pub target_hours: f64,
    pub rounding_increment: f64,
    pub total_hours: f64,
    pub day_confidence: ConfidenceGql,
    pub lines: Vec<TimesheetLineGql>,
    pub unattributed_hours: f64,
    pub unresolved: Vec<UnresolvedSignalGql>,
    pub blocks: Vec<AttributedBlockGql>,
}

impl ReconstructedDayGql {
    /// Build from the live reconstruction (has structured unresolved + blocks).
    pub fn from_reconstructed(
        day: ReconstructedDay,
        target_hours: f64,
        rounding_hours: f64,
        status: TimesheetStatus,
    ) -> Self {
        let mut lines: Vec<TimesheetLineGql> = day
            .allocations
            .into_iter()
            .map(|a: ProjectAllocation| TimesheetLineGql {
                gryzzly_project_id: Some(a.gryzzly_project_id),
                project_name: None,
                hours: a.hours,
                is_pinned: false,
                confidence: a.confidence.into(),
                source_refs: a.source_refs,
            })
            .collect();
        if day.unattributed_hours > 0.0 {
            lines.push(TimesheetLineGql {
                gryzzly_project_id: None,
                project_name: None,
                hours: day.unattributed_hours,
                is_pinned: false,
                confidence: ConfidenceGql::Low,
                source_refs: vec![],
            });
        }
        Self {
            date: day.date,
            status: status.into(),
            target_hours,
            rounding_increment: rounding_hours,
            total_hours: day.total_hours,
            day_confidence: day.day_confidence.into(),
            lines,
            unattributed_hours: day.unattributed_hours,
            unresolved: day.unresolved.into_iter().map(Into::into).collect(),
            blocks: day.blocks.into_iter().map(Into::into).collect(),
        }
    }

    /// Build from a persisted draft (blocks from `blocks_json`, unresolved from
    /// `unresolved_json`) — both written by `to_draft`, so a reload keeps the timeline AND
    /// the explanation of what stayed unattributed.
    pub fn from_draft(draft: TimesheetDraft, rounding_hours: f64) -> Self {
        let unattributed_hours: f64 = draft
            .lines
            .iter()
            .filter(|l| l.gryzzly_project_id.is_none())
            .map(|l| l.hours)
            .sum();
        let lines = draft
            .lines
            .into_iter()
            .map(|l: TimesheetDraftLine| TimesheetLineGql {
                gryzzly_project_id: l.gryzzly_project_id,
                project_name: l.project_name,
                hours: l.hours,
                is_pinned: l.is_pinned,
                confidence: l.confidence.into(),
                source_refs: l.source_refs,
            })
            .collect();
        // blocks_json is a best-effort display aid; ignore parse failures (empty timeline).
        let blocks = draft
            .blocks_json
            .as_deref()
            .and_then(parse_blocks_json)
            .unwrap_or_default();
        // Same contract for unresolved_json: best-effort, a day persisted before the
        // column existed simply has nothing to explain.
        let unresolved = draft
            .unresolved_json
            .as_deref()
            .and_then(parse_unresolved_json)
            .unwrap_or_default();
        Self {
            date: draft.date,
            status: draft.status.into(),
            target_hours: draft.target_hours,
            rounding_increment: rounding_hours,
            total_hours: draft.total_hours,
            day_confidence: draft.day_confidence.into(),
            lines,
            unattributed_hours,
            unresolved,
            blocks,
        }
    }
}

/// Timestamps in `blocks_json` / `unresolved_json` are `NaiveDateTime::to_string()`, which
/// prints a fractional part ONLY when the value has one. `%.f` consumes an optional
/// `.fraction`, so both shapes parse.
///
/// This is not hypothetical tidiness: an unresolved signal's `at` comes from a UTC instant
/// (`worklog_entries.logged_at`), so it effectively ALWAYS carries nanoseconds — a
/// second-only format string dropped every real signal and silently served an empty list on
/// every page load. Block times happen to be whole seconds (`and_hms_opt(h, m, 0)`), but
/// nothing enforces that and the failure mode there is a blank timeline, so both use this.
const PERSISTED_DT_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.f";

fn parse_persisted_dt(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, PERSISTED_DT_FORMAT).ok()
}

/// Parse the persisted blocks_json (written by Plan-1 `to_draft`) into display blocks.
/// Shape: [{start,end,gryzzlyProjectId,kind,hours,sourceRefs,originLabel}].
/// Returns None on any error.
///
/// `originLabel` is OPTIONAL on purpose: every day reconstructed before it existed has no
/// such key, and a required-key read there would return None for the whole array and blank
/// the timeline (the exact failure `unresolved_json` shipped with). Only `start`, `end` and
/// `kind` — without which a bar cannot be placed at all — stay mandatory.
fn parse_blocks_json(json: &str) -> Option<Vec<AttributedBlockGql>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = v.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for b in arr {
        let start = parse_persisted_dt(b.get("start")?.as_str()?)?;
        let end = parse_persisted_dt(b.get("end")?.as_str()?)?;
        let kind = match b.get("kind")?.as_str()? {
            "Meeting" => BlockKindGql::Meeting,
            "OutOfOffice" => BlockKindGql::OutOfOffice,
            _ => BlockKindGql::Work,
        };
        out.push(AttributedBlockGql {
            start_time: start,
            end_time: end,
            gryzzly_project_id: b.get("gryzzlyProjectId").and_then(|x| x.as_str()).map(String::from),
            kind,
            hours: b.get("hours").and_then(|x| x.as_f64()).unwrap_or(0.0),
            source_refs: b
                .get("sourceRefs")
                .and_then(|x| x.as_array())
                .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
                .unwrap_or_default(),
            origin_label: b.get("originLabel").and_then(|x| x.as_str()).map(String::from),
        });
    }
    Some(out)
}

/// Parse the persisted unresolved_json (written by `to_draft`) into display signals.
/// Shape: [{sourceRef,label,at}]. Returns None on any error.
fn parse_unresolved_json(json: &str) -> Option<Vec<UnresolvedSignalGql>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = v.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for u in arr {
        let at = parse_persisted_dt(u.get("at")?.as_str()?)?;
        out.push(UnresolvedSignalGql {
            source_ref: u.get("sourceRef")?.as_str()?.to_string(),
            label: u.get("label").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
            at,
        });
    }
    Some(out)
}

#[derive(InputObject)]
pub struct TimesheetLineInput {
    pub gryzzly_project_id: Option<ID>,
    pub hours: f64,
    pub is_pinned: bool,
}

impl From<TimesheetLineInput> for EditedLine {
    fn from(i: TimesheetLineInput) -> Self {
        EditedLine {
            gryzzly_project_id: i.gryzzly_project_id.map(|id| id.to_string()),
            hours: i.hours,
            is_pinned: i.is_pinned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use uuid::Uuid;

    #[test]
    fn from_draft_computes_unattributed_and_maps_lines() {
        let draft = TimesheetDraft {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            date: NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(),
            status: TimesheetStatus::Draft,
            target_hours: 7.5,
            total_hours: 7.5,
            day_confidence: Confidence::High,
            blocks_json: Some("[]".into()),
            unresolved_json: None,
            lanes_json: None,
            shares: vec![],
            lines: vec![
                TimesheetDraftLine {
                    id: Uuid::new_v4(),
                    gryzzly_project_id: Some("p1".into()),
                    project_name: Some("Proj 1".into()),
                    hours: 5.0,
                    is_pinned: false,
                    confidence: Confidence::High,
                    source_refs: vec!["wl:1".into()],
                },
                TimesheetDraftLine {
                    id: Uuid::new_v4(),
                    gryzzly_project_id: None,
                    project_name: None,
                    hours: 2.5,
                    is_pinned: false,
                    confidence: Confidence::Low,
                    source_refs: vec![],
                },
            ],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let gql = ReconstructedDayGql::from_draft(draft, 0.25);
        assert_eq!(gql.lines.len(), 2);
        assert!((gql.unattributed_hours - 2.5).abs() < 1e-9);
        assert!(matches!(gql.status, TimesheetStatusGql::Draft));
    }

    fn draft_with_unresolved_json(unresolved_json: Option<String>) -> TimesheetDraft {
        TimesheetDraft {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            date: NaiveDate::from_ymd_opt(2026, 6, 8).unwrap(),
            status: TimesheetStatus::Draft,
            target_hours: 7.5,
            total_hours: 7.5,
            day_confidence: Confidence::Low,
            blocks_json: None,
            unresolved_json,
            lanes_json: None,
            lines: vec![],
            shares: vec![],
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    /// A signal's `at` derives from `worklog_entries.logged_at`, a UTC instant, so the
    /// persisted value effectively always carries nanoseconds. A second-only format string
    /// dropped every real signal and served an empty list on every page load, while a
    /// whole-second fixture passed — so the fixture here is a value production can produce.
    #[test]
    fn from_draft_restores_persisted_unresolved_signals() {
        let json = r#"[{"sourceRef":"wl:abc","label":"revue de code","at":"2026-08-06 13:18:56.925353017"}]"#;
        let gql = ReconstructedDayGql::from_draft(
            draft_with_unresolved_json(Some(json.into())),
            0.25,
        );
        assert_eq!(gql.unresolved.len(), 1, "a reload must keep the explanation");
        assert_eq!(gql.unresolved[0].source_ref, "wl:abc");
        assert_eq!(gql.unresolved[0].label, "revue de code");
        assert_eq!(
            gql.unresolved[0].at.to_string(),
            "2026-08-06 13:18:56.925353017",
            "sub-second precision must survive the round trip, not be truncated away"
        );
    }

    /// Rows written before the fraction-tolerant format (and any block timestamp, which
    /// `reconstruct_day` builds on a whole second) must keep parsing.
    #[test]
    fn from_draft_restores_whole_second_unresolved_signals() {
        let json = r#"[{"sourceRef":"wl:abc","label":"revue de code","at":"2026-06-08 09:30:00"}]"#;
        let gql = ReconstructedDayGql::from_draft(
            draft_with_unresolved_json(Some(json.into())),
            0.25,
        );
        assert_eq!(gql.unresolved.len(), 1);
        assert_eq!(gql.unresolved[0].at.to_string(), "2026-06-08 09:30:00");
    }

    /// The same tolerance on the timeline: nothing enforces whole-second block times, and
    /// the failure mode there is a silently blank timeline.
    #[test]
    fn from_draft_parses_block_times_with_and_without_a_fraction() {
        let json = r#"[
            {"start":"2026-08-06 08:00:00","end":"2026-08-06 12:00:00","gryzzlyProjectId":"p1","kind":"Work","hours":4.0,"sourceRefs":["wl:1"]},
            {"start":"2026-08-06 13:18:56.925353017","end":"2026-08-06 17:00:00.5","gryzzlyProjectId":null,"kind":"Work","hours":3.5,"sourceRefs":["wl:2"]}
        ]"#;
        let mut draft = draft_with_unresolved_json(None);
        draft.blocks_json = Some(json.into());
        let gql = ReconstructedDayGql::from_draft(draft, 0.25);
        assert_eq!(gql.blocks.len(), 2, "one bad timestamp must not blank the timeline");
        assert_eq!(gql.blocks[1].start_time.to_string(), "2026-08-06 13:18:56.925353017");
    }

    /// The task title travels in `blocks_json` under `originLabel`; a reload must get it
    /// back, otherwise the timeline falls back to project-only bars.
    #[test]
    fn from_draft_restores_the_block_origin_label() {
        let json = r#"[
            {"start":"2026-08-06 08:00:00","end":"2026-08-06 12:00:00","gryzzlyProjectId":"p1","kind":"Work","hours":4.0,"sourceRefs":["wl:1"],"originLabel":"Refonte du portail client"},
            {"start":"2026-08-06 13:00:00","end":"2026-08-06 14:00:00","gryzzlyProjectId":"p1","kind":"Work","hours":1.0,"sourceRefs":["wl:2"],"originLabel":null}
        ]"#;
        let mut draft = draft_with_unresolved_json(None);
        draft.blocks_json = Some(json.into());
        let gql = ReconstructedDayGql::from_draft(draft, 0.25);
        assert_eq!(gql.blocks.len(), 2);
        assert_eq!(gql.blocks[0].origin_label.as_deref(), Some("Refonte du portail client"));
        assert_eq!(gql.blocks[1].origin_label, None, "an explicit null stays absent");
    }

    /// Every day reconstructed before `originLabel` existed has no such key. A missing
    /// optional key must yield `None` for that ONE block — never collapse the whole
    /// timeline to an empty list, which is exactly how `unresolved_json` broke.
    #[test]
    fn from_draft_parses_blocks_persisted_before_the_origin_label_existed() {
        let json = r#"[{"start":"2026-08-06 08:00:00","end":"2026-08-06 12:00:00","gryzzlyProjectId":"p1","kind":"Work","hours":4.0,"sourceRefs":["wl:1"]}]"#;
        let mut draft = draft_with_unresolved_json(None);
        draft.blocks_json = Some(json.into());
        let gql = ReconstructedDayGql::from_draft(draft, 0.25);
        assert_eq!(gql.blocks.len(), 1, "an old-shape payload must still render its timeline");
        assert_eq!(gql.blocks[0].origin_label, None);
        assert!((gql.blocks[0].hours - 4.0).abs() < 1e-9);
    }

    #[test]
    fn from_draft_tolerates_missing_or_broken_unresolved_json() {
        // Days persisted before migration 017 (NULL), and any future shape drift, must
        // degrade to "no explanation" rather than fail the whole query. `%.f` widens the
        // accepted timestamps, it does not make the parser accept junk.
        for json in [
            None,
            Some("not json".to_string()),
            Some(r#"[{"label":"x"}]"#.to_string()),
            Some(r#"[{"sourceRef":"wl:1","label":"x","at":"08/06/2026 13:18"}]"#.to_string()),
            Some(r#"[{"sourceRef":"wl:1","label":"x","at":"2026-08-06 13:18:56 UTC"}]"#.to_string()),
        ] {
            let gql = ReconstructedDayGql::from_draft(draft_with_unresolved_json(json), 0.25);
            assert!(gql.unresolved.is_empty());
        }
    }

    #[test]
    fn line_input_maps_to_edited_line() {
        let input = TimesheetLineInput {
            gryzzly_project_id: Some(ID("p1".into())),
            hours: 3.0,
            is_pinned: true,
        };
        let edited: EditedLine = input.into();
        assert_eq!(edited.gryzzly_project_id.as_deref(), Some("p1"));
        assert!(edited.is_pinned);
    }
}
