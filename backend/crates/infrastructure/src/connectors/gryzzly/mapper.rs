use application::services::{GryzzlyProject, GryzzlyTask};

use super::types::{RawGryzzlyProject, RawGryzzlyTask};

/// Map a raw API project DTO into the application-layer DTO.
pub fn map_project(raw: RawGryzzlyProject) -> GryzzlyProject {
    GryzzlyProject {
        id: raw.id,
        name: raw.name,
        customer_name: raw.customer_name,
        // If the API has no per-project archived flag, default to active.
        is_active: !raw.archived.unwrap_or(false),
    }
}

/// Map a raw API task DTO into the application-layer DTO.
/// `project_active` lets the caller fold project-level activeness in when the
/// task API exposes no per-task flag.
pub fn map_task(raw: RawGryzzlyTask, project_active: bool) -> GryzzlyTask {
    GryzzlyTask {
        id: raw.id,
        name: raw.name,
        project_id: raw.project_id,
        is_active: project_active && !raw.archived.unwrap_or(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connectors::gryzzly::types::{RawGryzzlyProject, RawGryzzlyTask};

    #[test]
    fn maps_active_project() {
        let raw = RawGryzzlyProject { id: "p1".into(), name: "Website".into(), customer_name: Some("Acme".into()), archived: Some(false) };
        let p = map_project(raw);
        assert_eq!(p.id, "p1");
        assert_eq!(p.name, "Website");
        assert_eq!(p.customer_name.as_deref(), Some("Acme"));
        assert!(p.is_active);
    }

    #[test]
    fn archived_project_is_inactive() {
        let raw = RawGryzzlyProject { id: "p2".into(), name: "Old".into(), customer_name: None, archived: Some(true) };
        assert!(!map_project(raw).is_active);
    }

    #[test]
    fn task_inactive_when_project_inactive() {
        let raw = RawGryzzlyTask { id: "t1".into(), name: "Dev".into(), project_id: "p1".into(), archived: Some(false) };
        assert!(!map_task(raw, false).is_active);
    }
}
