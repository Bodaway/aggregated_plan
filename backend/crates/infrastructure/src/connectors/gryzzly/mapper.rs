//! Wire DTOs to application DTOs, plus the tree-to-list flattening the catalog needs.

use application::services::{GryzzlyProject, GryzzlyTask};

use super::types::{RawGryzzlyProject, RawGryzzlyTask};

/// Recursion limit for the task tree. Mirrors the depth cap in
/// `scripts/gryzzly/export-catalog.console.js`; a cycle in the API's tree would
/// otherwise be unbounded.
pub(crate) const MAX_TASK_DEPTH: usize = 50;

/// A project is active only when Gryzzly says `status: "active"` and it is not
/// soft-deleted. Observed statuses: `active`, `done`.
pub(crate) fn map_project(raw: RawGryzzlyProject) -> GryzzlyProject {
    let is_active = raw.status.as_deref() == Some("active") && raw.deleted_at.is_none();
    GryzzlyProject {
        id: raw.id,
        name: raw.name.trim().to_string(),
        customer_name: raw
            .customer_name
            .map(|c| c.trim().to_string())
            .filter(|c| !c.is_empty()),
        is_active,
        status: raw.status,
    }
}

/// A task is active when it is neither finished nor deleted **in its own right**.
///
/// This deliberately does NOT fold in the owning project's state. Folding it was
/// what made a task on a closed project indistinguishable from one deleted in
/// Gryzzly: both arrived as `is_active = false`. The project's state now travels
/// separately as `project_status` on the catalog row.
pub(crate) fn map_task(raw: RawGryzzlyTask) -> GryzzlyTask {
    GryzzlyTask {
        id: raw.id,
        name: raw.name.trim().to_string(),
        project_id: raw.project_id.unwrap_or_default(),
        is_active: raw.completed_at.is_none() && raw.deleted_at.is_none(),
    }
}

/// Depth-first flatten of the nested `tasks` field into one list, parents before
/// children. Children inheriting `project_id` from the parent keeps rows
/// resolvable even where the API omits it.
pub(crate) fn flatten_tasks(
    tasks: Vec<RawGryzzlyTask>,
    fallback_project_id: &str,
    depth: usize,
) -> Vec<RawGryzzlyTask> {
    if depth > MAX_TASK_DEPTH {
        return Vec::new();
    }
    let mut out = Vec::new();
    for mut task in tasks {
        let children = task.tasks.take().unwrap_or_default();
        let project_id = task
            .project_id
            .clone()
            .unwrap_or_else(|| fallback_project_id.to_string());
        task.project_id = Some(project_id.clone());
        out.push(task);
        out.extend(flatten_tasks(children, &project_id, depth + 1));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(status: Option<&str>, deleted: Option<&str>) -> RawGryzzlyProject {
        RawGryzzlyProject {
            id: "p1".into(),
            name: " Website ".into(),
            customer_name: Some("Acme".into()),
            status: status.map(str::to_string),
            deleted_at: deleted.map(str::to_string),
        }
    }

    fn task(id: &str, completed: Option<&str>, deleted: Option<&str>) -> RawGryzzlyTask {
        RawGryzzlyTask {
            id: id.into(),
            name: format!(" {id} "),
            project_id: Some("p1".into()),
            is_container: Some(false),
            completed_at: completed.map(str::to_string),
            deleted_at: deleted.map(str::to_string),
            tasks: None,
        }
    }

    #[test]
    fn an_active_project_maps_active_and_trims_its_name() {
        let p = map_project(project(Some("active"), None));
        assert_eq!(p.id, "p1");
        assert_eq!(p.name, "Website");
        assert_eq!(p.customer_name.as_deref(), Some("Acme"));
        assert!(p.is_active);
    }

    #[test]
    fn a_done_project_maps_inactive() {
        assert!(!map_project(project(Some("done"), None)).is_active);
    }

    #[test]
    fn a_deleted_project_maps_inactive_even_when_active() {
        assert!(!map_project(project(Some("active"), Some("2026-01-01T00:00:00Z"))).is_active);
    }

    /// An absent status must not read as active: the old code defaulted an
    /// unknown flag to active and would have kept every project alive.
    #[test]
    fn a_project_without_status_maps_inactive() {
        assert!(!map_project(project(None, None)).is_active);
    }

    #[test]
    fn an_empty_customer_name_becomes_none() {
        let mut raw = project(Some("active"), None);
        raw.customer_name = Some("   ".into());
        assert_eq!(map_project(raw).customer_name, None);
    }

    #[test]
    fn an_open_task_is_active() {
        let t = map_task(task("t1", None, None));
        assert_eq!(t.id, "t1");
        assert_eq!(t.name, "t1");
        assert_eq!(t.project_id, "p1");
        assert!(t.is_active);
    }

    #[test]
    fn a_completed_task_is_inactive() {
        assert!(!map_task(task("t1", Some("2026-01-01T00:00:00Z"), None)).is_active);
    }

    #[test]
    fn a_deleted_task_is_inactive() {
        assert!(!map_task(task("t1", None, Some("2026-01-01T00:00:00Z"))).is_active);
    }

    /// THE semantic change. `is_active` used to fold in the project's state, which is
    /// exactly what made a task on a CLOSED project look like a DELETED task. A live
    /// task stays active regardless of its project; the project's state now travels
    /// separately, in `project_status`.
    #[test]
    fn a_live_task_stays_active_even_when_its_project_is_done() {
        let t = map_task(task("t1", None, None));
        assert!(t.is_active, "project state must no longer suppress a live task");
    }

    #[test]
    fn map_project_carries_the_raw_status_string() {
        let p = map_project(project(Some("done"), None));
        assert_eq!(p.status.as_deref(), Some("done"));
        assert!(!p.is_active);

        let p = map_project(project(Some("active"), None));
        assert_eq!(p.status.as_deref(), Some("active"));
        assert!(p.is_active);
    }

    #[test]
    fn map_project_status_is_none_when_absent() {
        assert_eq!(map_project(project(None, None)).status, None);
    }

    #[test]
    fn flatten_walks_the_whole_tree() {
        let mut parent = task("parent", None, None);
        parent.is_container = Some(true);
        let mut child = task("child", None, None);
        child.tasks = Some(vec![task("grandchild", None, None)]);
        parent.tasks = Some(vec![child]);

        let flat = flatten_tasks(vec![parent], "p1", 0);
        let ids: Vec<&str> = flat.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["parent", "child", "grandchild"]);
    }

    /// Containers stay in the catalog, matching import_catalog.py.
    #[test]
    fn flatten_keeps_container_tasks() {
        let mut parent = task("parent", None, None);
        parent.is_container = Some(true);
        parent.tasks = Some(vec![task("child", None, None)]);

        let flat = flatten_tasks(vec![parent], "p1", 0);
        assert_eq!(flat.len(), 2);
        assert_eq!(flat[0].is_container, Some(true));
    }

    #[test]
    fn a_child_without_project_id_inherits_it() {
        let mut parent = task("parent", None, None);
        let mut child = task("child", None, None);
        child.project_id = None;
        parent.tasks = Some(vec![child]);

        let flat = flatten_tasks(vec![parent], "fallback-project", 0);
        assert_eq!(flat[1].project_id.as_deref(), Some("p1"));
    }

    #[test]
    fn a_top_level_task_without_project_id_uses_the_fallback() {
        let mut orphan = task("orphan", None, None);
        orphan.project_id = None;
        let flat = flatten_tasks(vec![orphan], "fallback-project", 0);
        assert_eq!(flat[0].project_id.as_deref(), Some("fallback-project"));
    }

    #[test]
    fn flatten_stops_at_the_depth_cap() {
        // Build a chain deeper than the cap.
        let mut node = task("leaf", None, None);
        for i in 0..(MAX_TASK_DEPTH + 5) {
            let mut parent = task(&format!("n{i}"), None, None);
            parent.tasks = Some(vec![node]);
            node = parent;
        }
        let flat = flatten_tasks(vec![node], "p1", 0);
        assert!(flat.len() <= MAX_TASK_DEPTH + 1, "cap not enforced: {}", flat.len());
    }
}
