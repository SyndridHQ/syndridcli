use std::fmt;
use std::str::FromStr;

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::de::Error as _;
use thiserror::Error;

const MAX_IDENTIFIER_LENGTH: usize = 128;

/// Failure returned when a domain identifier is not a valid non-empty value.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum IdentifierError {
    /// The identifier contained no bytes.
    #[error("identifier must not be empty")]
    Empty,
    /// The identifier exceeded the domain limit.
    #[error("identifier exceeds the {MAX_IDENTIFIER_LENGTH}-byte limit")]
    TooLong,
}

macro_rules! domain_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
        pub struct $name(String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, IdentifierError> {
                let value = value.into();
                if value.is_empty() {
                    return Err(IdentifierError::Empty);
                }
                if value.len() > MAX_IDENTIFIER_LENGTH {
                    return Err(IdentifierError::TooLong);
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                Self::new(String::deserialize(deserializer)?).map_err(D::Error::custom)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = IdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

domain_id!(
    WorkflowId,
    "Opaque identifier for one orchestration workflow."
);
domain_id!(TaskId, "Opaque identifier for one bounded workflow task.");
domain_id!(
    AgentId,
    "Opaque identifier for one logical orchestration agent."
);
