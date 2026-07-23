use super::provider_connection::CredentialReference;
use super::provider_connection::CredentialSecret;
use std::error::Error;
use std::fmt;

/// Bounded failures for credential-store implementations.
///
/// Implementations must preserve the invariant that submitted secret values are never included
/// in an error or in diagnostic context.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CredentialStoreError {
    InvalidReference,
    NotFound,
    Unavailable,
    Rejected,
}

impl fmt::Display for CredentialStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidReference => "credential reference is invalid",
            Self::NotFound => "credential was not found",
            Self::Unavailable => "credential store is unavailable",
            Self::Rejected => "credential store rejected the operation",
        };
        formatter.write_str(message)
    }
}

impl Error for CredentialStoreError {}

/// Provider-neutral secret storage boundary for future OS-backed implementations.
///
/// Implementations own the storage mechanism. Callers pass opaque references in normal domain
/// records and receive a secret only at the provider-authentication boundary.
pub(super) trait CredentialStore: Send + Sync {
    fn store(
        &self,
        reference: &CredentialReference,
        secret: CredentialSecret,
    ) -> Result<(), CredentialStoreError>;

    fn retrieve(
        &self,
        reference: &CredentialReference,
    ) -> Result<CredentialSecret, CredentialStoreError>;

    fn delete(&self, reference: &CredentialReference) -> Result<(), CredentialStoreError>;

    fn contains(&self, reference: &CredentialReference) -> Result<bool, CredentialStoreError>;
}
