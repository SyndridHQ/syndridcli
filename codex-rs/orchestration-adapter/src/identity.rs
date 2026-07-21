use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

/// Maximum size of an adapter-supplied runtime identity.
pub const MAX_RUNTIME_ID_BYTES: usize = 128;

/// Failure returned for an invalid runtime identity.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum RuntimeIdentityError {
    #[error("runtime identity must not be empty")]
    Empty,
    #[error("runtime identity exceeds the 128-byte limit")]
    TooLong,
}

/// Opaque identity supplied by a future Codex runtime adapter.
///
/// O2 records this identity but never creates or resolves it. It is separate from the
/// orchestration [`codex_orchestration::AgentId`] assigned by Syndrid.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(transparent)]
pub struct RuntimeAgentId(String);

impl RuntimeAgentId {
    pub fn new(value: impl Into<String>) -> Result<Self, RuntimeIdentityError> {
        let value = value.into();
        if value.is_empty() {
            return Err(RuntimeIdentityError::Empty);
        }
        if value.len() > MAX_RUNTIME_ID_BYTES {
            return Err(RuntimeIdentityError::TooLong);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RuntimeAgentId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

impl fmt::Display for RuntimeAgentId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
