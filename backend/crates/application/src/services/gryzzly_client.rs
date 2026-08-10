use async_trait::async_trait;

use crate::errors::ConnectorError;

/// A Gryzzly project (read-only catalog). The project is contextual info only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GryzzlyProject {
    pub id: String,
    pub name: String,
    pub customer_name: Option<String>,
    pub is_active: bool,
    /// Raw status string from the API: `active` or `done`. Carried alongside the
    /// derived `is_active` on purpose — inferring "done" from `!is_active` works
    /// only while soft-deleted projects are filtered out, and a rendered badge
    /// should not depend on a two-step inference across two layers.
    pub status: Option<String>,
}

/// A Gryzzly task — a category of billable work within a project (NOT an aplan task).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GryzzlyTask {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub is_active: bool,
}

/// Read-only client for the Gryzzly internal RPC API (`POST https://api.gryzzly.io/<method>`).
/// Read-only is a hard constraint, not an accident: the cockpit never writes declarations.
#[async_trait]
pub trait GryzzlyClient: Send + Sync {
    /// List projects. When `active_only`, archived/closed projects are excluded.
    async fn fetch_projects(&self, active_only: bool) -> Result<Vec<GryzzlyProject>, ConnectorError>;

    /// List tasks belonging to the given project ids.
    async fn fetch_tasks(&self, project_ids: &[String]) -> Result<Vec<GryzzlyTask>, ConnectorError>;
}

/// Supplies the `Authorization` header value for the Gryzzly internal API.
///
/// Gryzzly issues no API keys. The only credential is the `remember_token`
/// session cookie minted by the Microsoft SSO login on `app.gryzzly.io`, which
/// has a fixed 7-day lifetime. This trait keeps *where that token comes from*
/// out of the HTTP client: infrastructure can read it from the local browser
/// cookie store, or take a hand-pasted value from configuration.
#[async_trait]
pub trait GryzzlyTokenSource: Send + Sync {
    /// The full header value, e.g. `User abc123…` — prefix included.
    async fn header_value(&self) -> Result<String, ConnectorError>;
}
