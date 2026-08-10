use async_graphql::SimpleObject;
use domain::types::GryzzlyCatalogEntry;

/// A Gryzzly catalog entry exposed via GraphQL — used by the `gryzzlyTasks` query.
#[derive(SimpleObject)]
pub struct GryzzlyTaskGql {
    pub gryzzly_task_id: String,
    pub name: String,
    pub gryzzly_project_id: String,
    pub project_name: String,
    pub customer_name: Option<String>,
    /// Status of the owning Gryzzly project (`active` | `done`), or null when
    /// unknown — a catalog row written before the column existed. Null renders as
    /// active: never as terminated.
    pub project_status: Option<String>,
}

impl From<GryzzlyCatalogEntry> for GryzzlyTaskGql {
    fn from(e: GryzzlyCatalogEntry) -> Self {
        Self {
            gryzzly_task_id: e.gryzzly_task_id,
            name: e.name,
            gryzzly_project_id: e.gryzzly_project_id,
            project_name: e.project_name,
            customer_name: e.customer_name,
            project_status: e.project_status,
        }
    }
}

/// The Gryzzly task assigned to a local task — embedded on `TaskGql.gryzzlyTask`.
///
/// Three stale states:
///   1. `stale = false` — catalog row exists and is active.
///   2. `stale = true`, `name = Some` — catalog row exists but has been soft-disabled.
///   3. `stale = true`, `name = None` — catalog row is absent (orphaned assignment).
#[derive(SimpleObject)]
pub struct AssignedGryzzlyTaskGql {
    pub gryzzly_task_id: String,
    pub name: Option<String>,
    pub project_name: Option<String>,
    pub stale: bool,
    /// Status of the owning Gryzzly project (`active` | `done`), or null when
    /// unknown. Independent of `stale`: a row can be both disabled in the catalog
    /// and owned by a closed project, and the two mean different things.
    pub project_status: Option<String>,
}

/// Pure resolver helper: maps (assignment id, optional catalog entry) to the stale-aware GQL type.
/// Kept as a free function so it can be unit-tested without a live database.
pub fn resolve_assigned(gid: String, entry: Option<GryzzlyCatalogEntry>) -> AssignedGryzzlyTaskGql {
    match entry {
        Some(e) if e.is_active => AssignedGryzzlyTaskGql {
            gryzzly_task_id: gid,
            name: Some(e.name),
            project_name: Some(e.project_name),
            stale: false,
            project_status: e.project_status,
        },
        Some(e) => AssignedGryzzlyTaskGql {
            gryzzly_task_id: gid,
            name: Some(e.name),
            project_name: Some(e.project_name),
            stale: true,
            project_status: e.project_status,
        },
        None => AssignedGryzzlyTaskGql {
            gryzzly_task_id: gid,
            name: None,
            project_name: None,
            stale: true,
            project_status: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::types::GryzzlyCatalogEntry;
    use uuid::Uuid;

    fn entry_with_status(is_active: bool, project_status: Option<&str>) -> GryzzlyCatalogEntry {
        let mut e = make_entry(is_active);
        e.project_status = project_status.map(str::to_string);
        e
    }

    #[test]
    fn an_active_row_on_a_done_project_is_not_stale_but_is_terminated() {
        let got = resolve_assigned("t1".into(), Some(entry_with_status(true, Some("done"))));
        assert!(!got.stale, "a closed project must not read as a missing row");
        assert_eq!(got.project_status.as_deref(), Some("done"));
    }

    /// The two markers are independent: a row can be both disabled in the catalog
    /// and owned by a closed project, and each means something different.
    #[test]
    fn a_disabled_row_keeps_its_project_status() {
        let got = resolve_assigned("t1".into(), Some(entry_with_status(false, Some("done"))));
        assert!(got.stale);
        assert_eq!(got.project_status.as_deref(), Some("done"));
    }

    #[test]
    fn an_orphaned_assignment_has_no_project_status() {
        let got = resolve_assigned("t1".into(), None);
        assert!(got.stale);
        assert_eq!(got.project_status, None);
    }

    #[test]
    fn an_unknown_project_status_is_carried_as_none() {
        let got = resolve_assigned("t1".into(), Some(entry_with_status(true, None)));
        assert!(!got.stale);
        assert_eq!(got.project_status, None);
    }

    fn make_entry(is_active: bool) -> GryzzlyCatalogEntry {
        GryzzlyCatalogEntry {
            id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            gryzzly_task_id: "GT-1".to_string(),
            name: "My Task".to_string(),
            gryzzly_project_id: "GP-1".to_string(),
            project_name: "My Project".to_string(),
            customer_name: None,
            is_active,
            project_status: None,
            last_synced_at: Utc::now(),
        }
    }

    #[test]
    fn stale_states() {
        // State 1: active entry → stale=false, name=Some
        let result = resolve_assigned("GT-1".to_string(), Some(make_entry(true)));
        assert!(!result.stale);
        assert_eq!(result.name.as_deref(), Some("My Task"));
        assert_eq!(result.project_name.as_deref(), Some("My Project"));

        // State 2: inactive entry → stale=true, name=Some
        let result = resolve_assigned("GT-1".to_string(), Some(make_entry(false)));
        assert!(result.stale);
        assert_eq!(result.name.as_deref(), Some("My Task"));
        assert_eq!(result.project_name.as_deref(), Some("My Project"));

        // State 3: missing entry → stale=true, name=None (no panic)
        let result = resolve_assigned("GT-1".to_string(), None);
        assert!(result.stale);
        assert!(result.name.is_none());
        assert!(result.project_name.is_none());
    }
}
