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
}

impl From<GryzzlyCatalogEntry> for GryzzlyTaskGql {
    fn from(e: GryzzlyCatalogEntry) -> Self {
        Self {
            gryzzly_task_id: e.gryzzly_task_id,
            name: e.name,
            gryzzly_project_id: e.gryzzly_project_id,
            project_name: e.project_name,
            customer_name: e.customer_name,
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
        },
        Some(e) => AssignedGryzzlyTaskGql {
            gryzzly_task_id: gid,
            name: Some(e.name),
            project_name: Some(e.project_name),
            stale: true,
        },
        None => AssignedGryzzlyTaskGql {
            gryzzly_task_id: gid,
            name: None,
            project_name: None,
            stale: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use domain::types::GryzzlyCatalogEntry;
    use uuid::Uuid;

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
