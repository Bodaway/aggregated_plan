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

    /// Build from a persisted draft (unresolved not persisted → empty; blocks from blocks_json).
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
        Self {
            date: draft.date,
            status: draft.status.into(),
            target_hours: draft.target_hours,
            rounding_increment: rounding_hours,
            total_hours: draft.total_hours,
            day_confidence: draft.day_confidence.into(),
            lines,
            unattributed_hours,
            unresolved: vec![],
            blocks,
        }
    }
}

/// Parse the persisted blocks_json (written by Plan-1 `to_draft`) into display blocks.
/// Shape: [{start,end,gryzzlyProjectId,kind,hours,sourceRefs}]. Returns None on any error.
fn parse_blocks_json(json: &str) -> Option<Vec<AttributedBlockGql>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = v.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    for b in arr {
        let start = NaiveDateTime::parse_from_str(b.get("start")?.as_str()?, "%Y-%m-%d %H:%M:%S").ok()?;
        let end = NaiveDateTime::parse_from_str(b.get("end")?.as_str()?, "%Y-%m-%d %H:%M:%S").ok()?;
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
