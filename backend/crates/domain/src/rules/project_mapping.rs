use std::collections::HashSet;

use crate::types::common::Confidence;
use crate::types::signal_mapping::{MappingKind, SignalMapping, SignalMappingId};

/// A raw signal, already stripped of I/O concerns, ready to be mapped to a project.
#[derive(Debug, Clone)]
pub enum RawSignal {
    Worklog {
        task_gryzzly_project_id: Option<String>,
    },
    Commit {
        repo_path: String,
        branch: String,
    },
    Meeting {
        subject: String,
        organizer: Option<String>,
        internal_project_id: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnmappedReason {
    TaskNotAssigned,
    NoRule,
    StaleMapping,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectResolution {
    Mapped {
        gryzzly_project_id: String,
        confidence: Confidence,
        source_rule_id: Option<SignalMappingId>,
    },
    Unmapped {
        reason: UnmappedReason,
        suggested: Option<SignalMappingId>,
    },
}

/// Resolve one signal to a Gryzzly project.
///
/// - Worklog: uses the task's already-snapshotted gryzzly_project_id (confidence High).
/// - Commit: matches enabled Branch rules (repo+branch) before RepoPath rules (repo only).
/// - Meeting: InternalProject rule, then MeetingOrganizer (exact), then MeetingSubject (substring).
///
/// A matched rule whose target project is absent from `live_project_ids` downgrades to
/// Unmapped{StaleMapping, suggested=rule_id}. No match → Unmapped{NoRule}.
pub fn resolve_signal_project(
    signal: &RawSignal,
    rules: &[SignalMapping],
    live_project_ids: &HashSet<String>,
) -> ProjectResolution {
    match signal {
        RawSignal::Worklog {
            task_gryzzly_project_id,
        } => match task_gryzzly_project_id {
            Some(pid) => finalize(pid.clone(), Confidence::High, None, live_project_ids),
            None => ProjectResolution::Unmapped {
                reason: UnmappedReason::TaskNotAssigned,
                suggested: None,
            },
        },
        RawSignal::Commit { repo_path, branch } => {
            // Branch rules (more specific) first, then RepoPath rules.
            if let Some(r) = best_match(rules, MappingKind::Branch, |m| {
                m.pattern == *repo_path
                    && m.branch_pattern.as_deref().map(|b| b == branch).unwrap_or(false)
            }) {
                return finalize(r.gryzzly_project_id.clone(), Confidence::High, Some(r.id), live_project_ids);
            }
            if let Some(r) = best_match(rules, MappingKind::RepoPath, |m| m.pattern == *repo_path) {
                return finalize(r.gryzzly_project_id.clone(), Confidence::Medium, Some(r.id), live_project_ids);
            }
            ProjectResolution::Unmapped { reason: UnmappedReason::NoRule, suggested: None }
        }
        RawSignal::Meeting {
            subject,
            organizer,
            internal_project_id,
        } => {
            if let Some(pid) = internal_project_id {
                if let Some(r) = best_match(rules, MappingKind::InternalProject, |m| m.pattern == *pid) {
                    return finalize(r.gryzzly_project_id.clone(), Confidence::High, Some(r.id), live_project_ids);
                }
            }
            if let Some(org) = organizer {
                if let Some(r) = best_match(rules, MappingKind::MeetingOrganizer, |m| {
                    m.pattern.eq_ignore_ascii_case(org)
                }) {
                    return finalize(r.gryzzly_project_id.clone(), Confidence::High, Some(r.id), live_project_ids);
                }
            }
            // Subject keyword: longest matching keyword wins.
            let subj_lower = subject.to_lowercase();
            let kw = rules
                .iter()
                .filter(|m| m.is_enabled && m.kind == MappingKind::MeetingSubject)
                .filter(|m| subj_lower.contains(&m.pattern.to_lowercase()))
                .max_by_key(|m| m.pattern.len());
            if let Some(r) = kw {
                return finalize(r.gryzzly_project_id.clone(), Confidence::Medium, Some(r.id), live_project_ids);
            }
            ProjectResolution::Unmapped { reason: UnmappedReason::NoRule, suggested: None }
        }
    }
}

fn best_match<'a>(
    rules: &'a [SignalMapping],
    kind: MappingKind,
    pred: impl Fn(&SignalMapping) -> bool,
) -> Option<&'a SignalMapping> {
    rules
        .iter()
        .filter(|m| m.is_enabled && m.kind == kind && pred(m))
        .max_by(|a, b| a.updated_at.cmp(&b.updated_at))
}

fn finalize(
    project_id: String,
    confidence: Confidence,
    rule_id: Option<SignalMappingId>,
    live_project_ids: &HashSet<String>,
) -> ProjectResolution {
    if live_project_ids.contains(&project_id) {
        ProjectResolution::Mapped {
            gryzzly_project_id: project_id,
            confidence,
            source_rule_id: rule_id,
        }
    } else {
        ProjectResolution::Unmapped {
            reason: UnmappedReason::StaleMapping,
            suggested: rule_id,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use uuid::Uuid;

    fn live(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn rule(kind: MappingKind, pattern: &str, project: &str) -> SignalMapping {
        SignalMapping {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            kind,
            pattern: pattern.to_string(),
            branch_pattern: None,
            gryzzly_project_id: project.to_string(),
            gryzzly_project_name: None,
            is_enabled: true,
            created_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
            updated_at: DateTime::<Utc>::from_timestamp(0, 0).unwrap(),
        }
    }

    #[test]
    fn worklog_uses_task_project_with_high_confidence() {
        let r = resolve_signal_project(
            &RawSignal::Worklog { task_gryzzly_project_id: Some("p1".into()) },
            &[],
            &live(&["p1"]),
        );
        assert_eq!(
            r,
            ProjectResolution::Mapped { gryzzly_project_id: "p1".into(), confidence: Confidence::High, source_rule_id: None }
        );
    }

    #[test]
    fn worklog_without_assignment_is_task_not_assigned() {
        let r = resolve_signal_project(
            &RawSignal::Worklog { task_gryzzly_project_id: None },
            &[],
            &live(&[]),
        );
        assert_eq!(r, ProjectResolution::Unmapped { reason: UnmappedReason::TaskNotAssigned, suggested: None });
    }

    #[test]
    fn meeting_internal_project_rule_wins_over_organizer() {
        let rules = vec![
            rule(MappingKind::InternalProject, "internal-42", "p_internal"),
            rule(MappingKind::MeetingOrganizer, "boss@corp.com", "p_org"),
        ];
        let r = resolve_signal_project(
            &RawSignal::Meeting {
                subject: "sync".into(),
                organizer: Some("boss@corp.com".into()),
                internal_project_id: Some("internal-42".into()),
            },
            &rules,
            &live(&["p_internal", "p_org"]),
        );
        match r {
            ProjectResolution::Mapped { gryzzly_project_id, .. } => assert_eq!(gryzzly_project_id, "p_internal"),
            other => panic!("expected Mapped, got {other:?}"),
        }
    }

    #[test]
    fn commit_branch_rule_beats_repo_rule() {
        let mut branch_rule = rule(MappingKind::Branch, "/home/me/repo", "p_branch");
        branch_rule.branch_pattern = Some("main".into());
        let rules = vec![branch_rule, rule(MappingKind::RepoPath, "/home/me/repo", "p_repo")];
        let r = resolve_signal_project(
            &RawSignal::Commit { repo_path: "/home/me/repo".into(), branch: "main".into() },
            &rules,
            &live(&["p_branch", "p_repo"]),
        );
        match r {
            ProjectResolution::Mapped { gryzzly_project_id, confidence, .. } => {
                assert_eq!(gryzzly_project_id, "p_branch");
                assert_eq!(confidence, Confidence::High);
            }
            other => panic!("expected Mapped, got {other:?}"),
        }
    }

    #[test]
    fn stale_project_downgrades_to_unmapped() {
        let rules = vec![rule(MappingKind::RepoPath, "/repo", "p_dead")];
        let r = resolve_signal_project(
            &RawSignal::Commit { repo_path: "/repo".into(), branch: "x".into() },
            &rules,
            &live(&["p_live"]), // p_dead not live
        );
        match r {
            ProjectResolution::Unmapped { reason, suggested } => {
                assert_eq!(reason, UnmappedReason::StaleMapping);
                assert!(suggested.is_some());
            }
            other => panic!("expected Unmapped/StaleMapping, got {other:?}"),
        }
    }

    #[test]
    fn no_matching_rule_is_no_rule() {
        let r = resolve_signal_project(
            &RawSignal::Commit { repo_path: "/unknown".into(), branch: "x".into() },
            &[],
            &live(&[]),
        );
        assert_eq!(r, ProjectResolution::Unmapped { reason: UnmappedReason::NoRule, suggested: None });
    }
}
