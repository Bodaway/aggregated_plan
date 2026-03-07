// Domain errors - implemented in Task 4

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("Not yet implemented")]
    NotImplemented,
}
