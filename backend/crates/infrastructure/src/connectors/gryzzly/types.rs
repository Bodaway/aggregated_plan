//! Wire DTOs for the Gryzzly internal API, deserializing only what the catalog
//! needs. The real project object carries 26 fields (including a large `metrics`
//! blob) and the task object 24; everything unused is dropped by serde.

use serde::Deserialize;

/// One project from `view/projects.list`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawGryzzlyProject {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub customer_name: Option<String>,
    /// Observed values: `active`, `done`. This is the only activeness signal —
    /// there is no `archived` field.
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

/// One task from `expandedProjectMetrics.get`, possibly with children.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawGryzzlyTask {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub project_id: Option<String>,
    /// A grouping task, which Gryzzly refuses declarations on. Kept in the
    /// catalog anyway, matching `scripts/gryzzly/import_catalog.py` — so nothing
    /// in production reads it, and the tests assert that choice holds.
    /// (`parent_id` exists on the wire too but is deliberately not deserialized:
    /// the tree is flattened via the nested `tasks` field, so nothing needs it.)
    #[allow(dead_code)]
    #[serde(default)]
    pub is_container: Option<bool>,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    /// Nested children. The API returns a tree, the catalog stores a flat list.
    #[serde(default)]
    pub tasks: Option<Vec<RawGryzzlyTask>>,
}

/// `expandedProjectMetrics.get` returns the whole project; only `tasks` is used.
/// `Default` so `post_payload` can treat a missing payload as "no tasks".
#[derive(Debug, Clone, Default, Deserialize)]
pub(crate) struct RawProjectMetrics {
    #[serde(default)]
    pub tasks: Option<Vec<RawGryzzlyTask>>,
}

/// The envelope every internal-API method wraps its result in.
///
/// `cursor` drives pagination on list methods and is absent elsewhere. `errors`
/// arrives on failures, which come with a non-2xx status as well.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Envelope<T> {
    pub ok: bool,
    // No `#[serde(default)]` here: on a generic field serde's derive would add a
    // spurious `T: Default` bound. A missing `Option` field already deserializes
    // to `None` — `parses_a_failure_with_an_errors_array` covers that.
    pub payload: Option<T>,
    #[serde(default)]
    pub errors: Option<Vec<String>>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shapes below are trimmed from fixtures captured against the live API.
    #[test]
    fn parses_a_real_projects_list_response() {
        let json = r#"{"ok":true,"cursor":null,"payload":[
            {"id":"p1","name":"Website","customer_name":"Acme","status":"active","deleted_at":null,
             "code":"","is_billable":true,"metrics":{"budget":{"budget_spent":0}}}
        ]}"#;
        let env: Envelope<Vec<RawGryzzlyProject>> = serde_json::from_str(json).unwrap();
        assert!(env.ok);
        assert!(env.cursor.is_none());
        let payload = env.payload.unwrap();
        assert_eq!(payload.len(), 1);
        assert_eq!(payload[0].status.as_deref(), Some("active"));
        assert_eq!(payload[0].customer_name.as_deref(), Some("Acme"));
    }

    #[test]
    fn parses_a_failure_with_an_errors_array() {
        let json = r#"{"ok":false,"errors":["decoding: invalid_argument: limit (out of range, max=500)"]}"#;
        let env: Envelope<Vec<RawGryzzlyProject>> = serde_json::from_str(json).unwrap();
        assert!(!env.ok);
        assert_eq!(env.errors.unwrap().len(), 1);
        assert!(env.payload.is_none());
    }

    #[test]
    fn parses_a_metrics_response_and_keeps_only_tasks() {
        let json = r#"{"ok":true,"payload":{"id":"p1","name":"Website","tasks":[
            {"id":"t1","name":"Pilotage","project_id":"p1","parent_id":null,"is_container":false,
             "completed_at":null,"deleted_at":null,"planned_duration":63000}
        ]}}"#;
        let env: Envelope<RawProjectMetrics> = serde_json::from_str(json).unwrap();
        let tasks = env.payload.unwrap().tasks.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "t1");
        assert_eq!(tasks[0].is_container, Some(false));
    }

    /// A cursor is a plain string on the wire, not an object.
    #[test]
    fn parses_a_non_null_cursor() {
        let json = r#"{"ok":true,"cursor":"fecdfc2c-2d53-490d-ac3a-4c09c75c4dc1","payload":[]}"#;
        let env: Envelope<Vec<RawGryzzlyProject>> = serde_json::from_str(json).unwrap();
        assert_eq!(env.cursor.as_deref(), Some("fecdfc2c-2d53-490d-ac3a-4c09c75c4dc1"));
    }
}
