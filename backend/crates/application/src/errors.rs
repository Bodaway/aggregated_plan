use domain::types::Source;

/// Top-level application error type that wraps domain, repository, and connector errors.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Domain error: {0}")]
    Domain(#[from] domain::errors::DomainError),

    #[error("Repository error: {0}")]
    Repository(#[from] RepositoryError),

    #[error("Connector error: {connector_source:?} -- {message}")]
    Connector {
        connector_source: Source,
        message: String,
    },

    #[error("Configuration error: {0}")]
    Configuration(String),

    #[error("Not found: {0}")]
    NotFound(String),

    /// A short reference matched several rows, so acting on it would be a guess.
    /// Carries the whole "which one did you mean" message, candidates included,
    /// because callers surface it verbatim — the CLI prints it and exits 3.
    #[error("{0}")]
    Ambiguous(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),
}

/// Error type for repository operations (database, serialization).
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("Database error: {0}")]
    Database(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Error type for external connector operations (Jira, Outlook, Excel).
#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("HTTP error: {status} -- {message}")]
    Http { status: u16, message: String },

    #[error("Authentication failed for {service}")]
    AuthFailed { service: String },

    #[error("Network unreachable: {0}")]
    NetworkError(String),

    #[error("Parsing error: {0}")]
    ParseError(String),

    /// The local environment is not set up for this connector: no browser cookie
    /// store found, the OS keyring is unavailable, or a stored credential expired.
    /// Distinct from `AuthFailed`, which means the remote end rejected us — and
    /// which cannot carry a detail message, since its Display is fixed.
    #[error("Configuration error: {0}")]
    Configuration(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_connector_error_displays_its_message() {
        let e = ConnectorError::Configuration("no cookie found".to_string());
        assert_eq!(e.to_string(), "Configuration error: no cookie found");
    }
}
