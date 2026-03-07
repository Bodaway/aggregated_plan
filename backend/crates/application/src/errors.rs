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
}
