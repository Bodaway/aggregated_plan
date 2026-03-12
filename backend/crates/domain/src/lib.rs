pub mod types;
pub mod rules;
pub mod errors;

pub type DomainResult<T> = Result<T, errors::DomainError>;
