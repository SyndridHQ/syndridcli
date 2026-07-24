use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;

const MAX_ID_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 256;

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodexAccountProfileId(String);

impl CodexAccountProfileId {
    pub fn new(value: impl Into<String>) -> Result<Self, CodexAccountProfileError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > MAX_ID_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(CodexAccountProfileError::InvalidCodexAccountProfileId);
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for CodexAccountProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CodexAccountProfileId")
            .field(&self.0)
            .finish()
    }
}
impl fmt::Display for CodexAccountProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Serialize for CodexAccountProfileId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for CodexAccountProfileId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexAccountProfileState {
    Unconfigured,
    AuthenticationPending,
    Connected,
    ReauthenticationRequired,
    Disabled,
    Invalid,
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CodexAccountConnectionMetadata {
    pub connection_id: String,
    pub profile_id: CodexAccountProfileId,
    pub provider_id: String,
    pub label: String,
    pub state: CodexAccountProfileState,
    pub account_email: Option<String>,
    pub account_id: Option<String>,
    pub plan_label: Option<String>,
    pub enabled: bool,
    pub validation: ConnectionValidationStatus,
    pub last_authenticated_at: Option<u64>,
    pub last_validated_at: Option<u64>,
    pub credential_reference: String,
}

use super::provider_connection::ConnectionValidationStatus;

impl fmt::Debug for CodexAccountConnectionMetadata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CodexAccountConnectionMetadata")
            .field("connection_id", &self.connection_id)
            .field("profile_id", &self.profile_id)
            .field("provider_id", &self.provider_id)
            .field("label", &self.label)
            .field("state", &self.state)
            .field("account_email", &"<redacted>")
            .field("account_id", &"<redacted>")
            .field("plan_label", &self.plan_label)
            .field("enabled", &self.enabled)
            .field("validation", &self.validation)
            .field("credential_reference", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CodexAccountProfileRegistry {
    profiles: BTreeMap<CodexAccountProfileId, CodexAccountConnectionMetadata>,
}

impl CodexAccountProfileRegistry {
    pub fn insert(
        &mut self,
        metadata: CodexAccountConnectionMetadata,
    ) -> Result<(), CodexAccountProfileError> {
        if metadata.provider_id != "codex"
            || metadata.connection_id.trim().is_empty()
            || metadata.credential_reference.trim().is_empty()
            || metadata.label.trim().is_empty()
            || metadata.label.len() > MAX_LABEL_BYTES
        {
            return Err(CodexAccountProfileError::InvalidAccountState);
        }
        if self.profiles.contains_key(&metadata.profile_id)
            || self
                .profiles
                .values()
                .any(|item| item.connection_id == metadata.connection_id)
        {
            return Err(CodexAccountProfileError::DuplicateCodexAccountProfile);
        }
        self.profiles.insert(metadata.profile_id.clone(), metadata);
        Ok(())
    }
    pub fn get(&self, id: &CodexAccountProfileId) -> Option<&CodexAccountConnectionMetadata> {
        self.profiles.get(id)
    }
    pub fn profiles(&self) -> impl Iterator<Item = &CodexAccountConnectionMetadata> {
        self.profiles.values()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexAccountProfileError {
    InvalidCodexAccountProfileId,
    DuplicateCodexAccountProfile,
    InvalidAccountState,
}
impl fmt::Display for CodexAccountProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCodexAccountProfileId => "Codex account profile ID is invalid",
            Self::DuplicateCodexAccountProfile => "Codex account profile already exists",
            Self::InvalidAccountState => "Codex account profile state is invalid",
        })
    }
}
impl std::error::Error for CodexAccountProfileError {}

#[cfg(test)]
#[path = "codex_account_profile_tests.rs"]
mod codex_account_profile_tests;
