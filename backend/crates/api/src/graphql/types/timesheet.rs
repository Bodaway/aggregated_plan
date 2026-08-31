use async_graphql::{SimpleObject, ID};
use chrono::{NaiveDate, NaiveDateTime};

use domain::rules::presence::{Lane, LaneKey};
use domain::rules::quarters::{quarters, QuarterAllocation};
use domain::rules::reconstruction::{
    OutsideWork, ProjectAllocation, ReconstructedDay, ReconstructionConfig, UnresolvedSignal,
};
use domain::types::{
    QuarterShareRow, SignalMapping, TimesheetDraft, TimesheetDraftLine, TimesheetStatus,
};

use super::enums::{ConfidenceGql, MappingKindGql, TimesheetStatusGql};

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

/// One stretch of a lane, in local minutes from midnight. Minutes rather than
/// timestamps because a lane is drawn against the day's own grid, and a client that has
/// to parse datetimes to position a bar will get the timezone wrong eventually.
#[derive(SimpleObject)]
pub struct LaneIntervalGql {
    pub start_min: i32,
    pub end_min: i32,
}

/// One task's presence across the day. Lanes overlap — that is the concurrent view.
#[derive(SimpleObject)]
pub struct LaneGql {
    pub lane_key: String,
    /// The plan task this lane is about, when it has one. Meeting and repo lanes carry
    /// no task, so they have no Gryzzly snapshot a client could correct — which is what
    /// tells the timesheet screen whether to offer a reassignment control on the row.
    pub task_id: Option<ID>,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub intervals: Vec<LaneIntervalGql>,
    pub outside_minutes: i32,
}

impl From<Lane> for LaneGql {
    fn from(l: Lane) -> Self {
        Self {
            task_id: l.key.task_id().map(|id| ID(id.to_string())),
            lane_key: l.key.as_key(),
            label: l.label,
            gryzzly_project_id: l.gryzzly_project_id,
            intervals: l
                .intervals
                .into_iter()
                .map(|(s, e)| LaneIntervalGql { start_min: s as i32, end_min: e as i32 })
                .collect(),
            outside_minutes: l.outside_minutes as i32,
        }
    }
}

/// What one lane declares inside one quarter, with the weight it was derived from.
#[derive(SimpleObject)]
pub struct QuarterShareGql {
    pub lane_key: String,
    pub task_id: Option<ID>,
    pub label: String,
    pub gryzzly_project_id: Option<String>,
    pub presence_minutes: i32,
    pub hours: f64,
    pub is_pinned: bool,
}

/// A quarter-day and its arbitration. `shares` always sums to `declarable_hours`.
#[derive(SimpleObject)]
pub struct QuarterGql {
    pub index: i32,
    pub start_min: i32,
    pub end_min: i32,
    pub hours: f64,
    pub ooo_hours: f64,
    pub declarable_hours: f64,
    pub confidence: ConfidenceGql,
    pub shares: Vec<QuarterShareGql>,
}

impl From<QuarterAllocation> for QuarterGql {
    fn from(q: QuarterAllocation) -> Self {
        Self {
            index: q.quarter.index as i32,
            start_min: q.quarter.start_min as i32,
            end_min: q.quarter.end_min as i32,
            hours: q.quarter.hours,
            ooo_hours: q.ooo_hours,
            declarable_hours: q.declarable_hours,
            confidence: q.confidence.into(),
            shares: q
                .shares
                .into_iter()
                .map(|s| QuarterShareGql {
                    lane_key: s.lane.as_key(),
                    task_id: s.lane.task_id().map(|id| ID(id.to_string())),
                    label: s.label,
                    gryzzly_project_id: s.gryzzly_project_id,
                    presence_minutes: s.presence_minutes as i32,
                    hours: s.hours,
                    is_pinned: s.is_pinned,
                })
                .collect(),
        }
    }
}

/// Evidence that fell outside the working windows — surfaced so the user can decide,
/// rather than dropped where nobody can see it.
#[derive(SimpleObject)]
pub struct OutsideWorkGql {
    pub lane_key: String,
    pub label: String,
    pub minutes: i32,
}

impl From<OutsideWork> for OutsideWorkGql {
    fn from(o: OutsideWork) -> Self {
        Self { lane_key: o.lane.as_key(), label: o.label, minutes: o.minutes as i32 }
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
    pub lanes: Vec<LaneGql>,
    pub quarters: Vec<QuarterGql>,
    pub outside_workday: Vec<OutsideWorkGql>,
}

impl ReconstructedDayGql {
    /// Build from the live reconstruction, which carries the lanes and the quarters.
    pub fn from_reconstructed(
        day: ReconstructedDay,
        cfg: &ReconstructionConfig,
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
            target_hours: cfg.daily_target_hours,
            rounding_increment: cfg.rounding_hours,
            total_hours: day.total_hours,
            day_confidence: day.day_confidence.into(),
            lines,
            unattributed_hours: day.unattributed_hours,
            unresolved: day.unresolved.into_iter().map(Into::into).collect(),
            lanes: day.lanes.into_iter().map(Into::into).collect(),
            quarters: day.quarters.into_iter().map(Into::into).collect(),
            outside_workday: day.outside_workday.into_iter().map(Into::into).collect(),
        }
    }

    /// Build from a persisted draft: lines and quarter shares from their tables, the
    /// evidence view from `lanes_json`.
    ///
    /// The quarters are rebuilt from the share rows and the configured windows. Two
    /// fields cannot be: a quarter's own confidence and its out-of-office hours are
    /// properties of the evidence, not of the decision, so a reloaded day reports the
    /// DAY's confidence on each quarter and no off-time. Reconstructing refreshes both.
    pub fn from_draft(draft: TimesheetDraft, cfg: &ReconstructionConfig) -> Self {
        let unattributed_hours: f64 = draft
            .lines
            .iter()
            .filter(|l| l.gryzzly_project_id.is_none())
            .map(|l| l.hours)
            .sum();
        let day_confidence: ConfidenceGql = draft.day_confidence.into();
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

        let quarters = quarters(cfg)
            .into_iter()
            .map(|q| {
                let shares: Vec<QuarterShareGql> = draft
                    .shares
                    .iter()
                    .filter(|s| s.quarter_index == q.index)
                    .map(|s: &QuarterShareRow| QuarterShareGql {
                        lane_key: s.lane_key.clone(),
                        task_id: s.task_id.map(|id| ID(id.to_string())),
                        label: s.label.clone(),
                        gryzzly_project_id: s.gryzzly_project_id.clone(),
                        presence_minutes: s.presence_minutes as i32,
                        hours: s.hours,
                        is_pinned: s.is_pinned,
                    })
                    .collect();
                let declared: f64 = shares.iter().map(|s| s.hours).sum();
                QuarterGql {
                    index: q.index as i32,
                    start_min: q.start_min as i32,
                    end_min: q.end_min as i32,
                    hours: q.hours,
                    ooo_hours: 0.0,
                    declarable_hours: declared,
                    confidence: day_confidence,
                    shares,
                }
            })
            .collect();

        // lanes_json is a best-effort display aid: a day persisted before the column
        // existed shows no evidence view rather than failing the whole query.
        let lanes = draft.lanes_json.as_deref().and_then(parse_lanes_json).unwrap_or_default();
        // Same contract for unresolved_json.
        let unresolved = draft
            .unresolved_json
            .as_deref()
            .and_then(parse_unresolved_json)
            .unwrap_or_default();
        Self {
            date: draft.date,
            status: draft.status.into(),
            target_hours: draft.target_hours,
            rounding_increment: cfg.rounding_hours,
            total_hours: draft.total_hours,
            day_confidence,
            lines,
            unattributed_hours,
            unresolved,
            lanes,
            quarters,
            // Derived from the evidence, not persisted: a reload has nothing to report
            // until the day is reconstructed again.
            outside_workday: vec![],
        }
    }
}

/// Parse the persisted lanes_json into the evidence view.
/// Shape: `[{laneKey,label,gryzzlyProjectId,intervals:[[start,end]],outsideMinutes}]`.
///
/// Every field except `laneKey` and `intervals` is optional on read: a lane missing its
/// label is still a lane worth drawing, and the failure this tolerance exists to prevent
/// — one absent key blanking the entire view — has already shipped once here.
fn parse_lanes_json(json: &str) -> Option<Vec<LaneGql>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = v.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for l in arr {
        let lane_key = l.get("laneKey")?.as_str()?.to_string();
        let intervals = l
            .get("intervals")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|pair| {
                        let p = pair.as_array()?;
                        Some(LaneIntervalGql {
                            start_min: p.first()?.as_i64()? as i32,
                            end_min: p.get(1)?.as_i64()? as i32,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(LaneGql {
            task_id: LaneKey::parse(&lane_key)
                .and_then(|k| k.task_id())
                .map(|id| ID(id.to_string())),
            label: l
                .get("label")
                .and_then(|x| x.as_str())
                .unwrap_or(&lane_key)
                .to_string(),
            lane_key,
            gryzzly_project_id: l.get("gryzzlyProjectId").and_then(|x| x.as_str()).map(String::from),
            intervals,
            outside_minutes: l.get("outsideMinutes").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
        });
    }
    Some(out)
}


/// Timestamps in `unresolved_json` are `NaiveDateTime::to_string()`, which
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::types::Confidence;
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
        let gql = ReconstructedDayGql::from_draft(draft, &ReconstructionConfig::default());
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
            &ReconstructionConfig::default(),
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
            &ReconstructionConfig::default(),
        );
        assert_eq!(gql.unresolved.len(), 1);
        assert_eq!(gql.unresolved[0].at.to_string(), "2026-06-08 09:30:00");
    }




    #[test]
    fn parse_lanes_json_exposes_the_task_id_of_task_lanes_only() {
        // The picker on a lane row can only reassign a lane that HAS a task; a meeting or
        // an unmatched repo has no task snapshot to write the Gryzzly project onto.
        let task = Uuid::new_v4();
        let json = format!(
            r#"[{{"laneKey":"task:{task}","label":"HUD","intervals":[[540,600]]}},
                {{"laneKey":"src:mtg:42","label":"Daily","intervals":[]}},
                {{"laneKey":"unattributed","label":"reste","intervals":[]}}]"#
        );
        let lanes = parse_lanes_json(&json).expect("well-formed lanes parse");
        assert_eq!(lanes[0].task_id, Some(ID(task.to_string())));
        assert_eq!(lanes[1].task_id, None);
        assert_eq!(lanes[2].task_id, None);
    }

    #[test]
    fn parse_lanes_json_leaves_task_id_empty_on_a_malformed_key() {
        // Tolerance, same contract as the rest of the parser: a lane key that is not a
        // valid `task:<uuid>` still draws, it just cannot be reassigned.
        let json = r#"[{"laneKey":"task:not-a-uuid","label":"x","intervals":[]}]"#;
        let lanes = parse_lanes_json(json).expect("well-formed lanes parse");
        assert_eq!(lanes[0].task_id, None);
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
            let gql = ReconstructedDayGql::from_draft(draft_with_unresolved_json(json), &ReconstructionConfig::default());
            assert!(gql.unresolved.is_empty());
        }
    }

}
