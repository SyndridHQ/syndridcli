use super::credential_store::CredentialStore;
use codex_login::AuthDotJson;
use codex_login::TokenData;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::NamedTempFile;

const MAX_ID_BYTES: usize = 128;
const MAX_LABEL_BYTES: usize = 256;
const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_PROFILES: usize = 32;
const SCHEMA_VERSION: u32 = 1;
const CODEX_PROVIDER_ID: &str = "codex";
const CREDENTIAL_ENVELOPE_VERSION: u32 = 1;
const MAX_CREDENTIAL_ENVELOPE_BYTES: usize = 64 * 1024;

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
    pub schema_version: u32,
}

impl CodexAccountProfileState {
    pub fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Unconfigured, Self::AuthenticationPending)
                | (Self::AuthenticationPending, Self::Connected)
                | (Self::AuthenticationPending, Self::Invalid)
                | (Self::Connected, Self::ReauthenticationRequired)
                | (Self::Connected, Self::Disabled)
                | (Self::ReauthenticationRequired, Self::AuthenticationPending)
                | (Self::Disabled, Self::AuthenticationPending)
                | (Self::Connected, Self::Unconfigured)
                | (Self::Invalid, Self::Unconfigured)
        )
    }
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
            .field("schema_version", &self.schema_version)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CodexAccountProfileRegistry {
    pub schema_version: u32,
    profiles: BTreeMap<CodexAccountProfileId, CodexAccountConnectionMetadata>,
}

impl Default for CodexAccountProfileRegistry {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            profiles: BTreeMap::new(),
        }
    }
}

impl CodexAccountProfileRegistry {
    pub fn insert(
        &mut self,
        metadata: CodexAccountConnectionMetadata,
    ) -> Result<(), CodexAccountProfileError> {
        CodexAccountProfileId::new(metadata.connection_id.clone())?;
        if metadata.schema_version != SCHEMA_VERSION
            || metadata.provider_id != CODEX_PROVIDER_ID
            || metadata.connection_id.trim().is_empty()
            || metadata.credential_reference.trim().is_empty()
            || metadata.credential_reference
                != Self::credential_reference_for(&metadata.connection_id)?
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
            || self
                .profiles
                .values()
                .any(|item| item.credential_reference == metadata.credential_reference)
        {
            return Err(
                if self
                    .profiles
                    .values()
                    .any(|item| item.credential_reference == metadata.credential_reference)
                {
                    CodexAccountProfileError::DuplicateCredentialReference
                } else {
                    CodexAccountProfileError::DuplicateCodexAccountProfile
                },
            );
        }
        if self.profiles.len() >= MAX_PROFILES {
            return Err(CodexAccountProfileError::TooManyProfiles);
        }
        self.profiles.insert(metadata.profile_id.clone(), metadata);
        Ok(())
    }
    pub fn get(&self, id: &CodexAccountProfileId) -> Option<&CodexAccountConnectionMetadata> {
        self.profiles.get(id)
    }
    pub fn get_connection(&self, connection_id: &str) -> Option<&CodexAccountConnectionMetadata> {
        self.profiles
            .values()
            .find(|profile| profile.connection_id == connection_id)
    }
    pub fn get_mut(
        &mut self,
        id: &CodexAccountProfileId,
    ) -> Option<&mut CodexAccountConnectionMetadata> {
        self.profiles.get_mut(id)
    }
    pub fn profiles(&self) -> impl Iterator<Item = &CodexAccountConnectionMetadata> {
        self.profiles.values()
    }

    pub fn load(path: &Path) -> Result<Self, CodexAccountProfileError> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err(CodexAccountProfileError::RegistryUnavailable),
        };
        if bytes.len() > MAX_FILE_BYTES {
            return Err(CodexAccountProfileError::RegistryTooLarge);
        }
        let registry: Self = serde_json::from_slice(&bytes)
            .map_err(|_| CodexAccountProfileError::RegistryMalformed)?;
        if registry.schema_version != SCHEMA_VERSION {
            return Err(CodexAccountProfileError::UnsupportedSchemaVersion);
        }
        let mut checked = Self::default();
        for profile in registry.profiles.into_values() {
            checked.insert(profile)?;
        }
        Ok(checked)
    }

    pub fn save(&self, path: &Path) -> Result<(), CodexAccountProfileError> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|_| CodexAccountProfileError::AtomicWriteFailed)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(CodexAccountProfileError::RegistryTooLarge);
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|_| CodexAccountProfileError::AtomicWriteFailed)?;
        }
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        let mut temporary = NamedTempFile::new_in(parent)
            .map_err(|_| CodexAccountProfileError::AtomicWriteFailed)?;
        std::io::Write::write_all(&mut temporary, &bytes)
            .map_err(|_| CodexAccountProfileError::AtomicWriteFailed)?;
        temporary
            .persist(path)
            .map_err(|_| CodexAccountProfileError::AtomicWriteFailed)?;
        Ok(())
    }

    pub fn credential_reference_for(
        connection_id: &str,
    ) -> Result<String, CodexAccountProfileError> {
        CodexAccountProfileId::new(connection_id.to_string())?;
        Ok(format!("codex-account-{connection_id}"))
    }

    pub fn transition(
        &mut self,
        id: &CodexAccountProfileId,
        next: CodexAccountProfileState,
    ) -> Result<(), CodexAccountProfileError> {
        let profile = self
            .profiles
            .get_mut(id)
            .ok_or(CodexAccountProfileError::UnknownCodexAccountProfile)?;
        if !profile.state.can_transition_to(next) {
            return Err(CodexAccountProfileError::UnsupportedStateTransition);
        }
        profile.state = next;
        Ok(())
    }
}

#[derive(Clone)]
pub struct CodexAccountStore {
    path: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl fmt::Debug for CodexAccountStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexAccountStore")
            .field("path", &self.path)
            .finish()
    }
}

impl CodexAccountStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn load(&self) -> Result<CodexAccountProfileRegistry, CodexAccountProfileError> {
        CodexAccountProfileRegistry::load(&self.path)
    }

    pub fn save(
        &self,
        registry: &CodexAccountProfileRegistry,
    ) -> Result<(), CodexAccountProfileError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| CodexAccountProfileError::AtomicWriteFailed)?;
        registry.save(&self.path)
    }
}

/// Versioned secret payload stored for one explicitly selected Codex connection.
/// Token fields remain private to the authentication boundary.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexCredentialEnvelope {
    schema_version: u32,
    credential_kind: CodexCredentialKind,
    payload: TokenData,
}

/// In-memory credentials for one selected Codex connection.
///
/// This type deliberately has no serialization implementation. It is created only after the
/// versioned envelope has crossed the credential-store boundary and is consumed by one scoped
/// request.
pub(crate) struct CodexCredentialSnapshot {
    access_token: String,
    account_id: Option<String>,
}

impl fmt::Debug for CodexCredentialSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexCredentialSnapshot")
            .field("has_access_token", &true)
            .field("has_account_id", &self.account_id.is_some())
            .finish()
    }
}

impl CodexCredentialSnapshot {
    pub(crate) fn access_token(&self) -> &str {
        &self.access_token
    }

    pub(crate) fn account_id(&self) -> Option<&str> {
        self.account_id.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CodexCredentialKind {
    #[serde(rename = "chatgpt_oauth")]
    ChatgptOAuth,
}

impl fmt::Debug for CodexCredentialEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CodexCredentialEnvelope")
            .field("schema_version", &self.schema_version)
            .field("credential_kind", &self.credential_kind)
            .field("has_access_token", &!self.payload.access_token.is_empty())
            .field("has_refresh_token", &!self.payload.refresh_token.is_empty())
            .field("has_id_token", &!self.payload.id_token.raw_jwt.is_empty())
            .field("has_account_id", &self.payload.account_id.is_some())
            .finish()
    }
}

impl fmt::Display for CodexCredentialEnvelope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Codex credential envelope (<redacted>)")
    }
}

impl CodexCredentialEnvelope {
    fn from_auth(auth: &AuthDotJson) -> Result<Self, CodexAccountProfileError> {
        let payload = auth
            .tokens
            .clone()
            .ok_or(CodexAccountProfileError::MissingRequiredCredentialField)?;
        if payload.access_token.trim().is_empty() || payload.refresh_token.trim().is_empty() {
            return Err(CodexAccountProfileError::MissingRequiredCredentialField);
        }
        let envelope = Self {
            schema_version: CREDENTIAL_ENVELOPE_VERSION,
            credential_kind: CodexCredentialKind::ChatgptOAuth,
            payload,
        };
        envelope.serialized()?;
        Ok(envelope)
    }

    pub fn parse(value: &str) -> Result<Self, CodexAccountProfileError> {
        if value.len() > MAX_CREDENTIAL_ENVELOPE_BYTES {
            return Err(CodexAccountProfileError::CredentialEnvelopeTooLarge);
        }
        let envelope: Self = serde_json::from_str(value)
            .map_err(|_| CodexAccountProfileError::InvalidCredentialEnvelope)?;
        if envelope.schema_version != CREDENTIAL_ENVELOPE_VERSION {
            return Err(CodexAccountProfileError::UnsupportedCredentialEnvelopeVersion);
        }
        if envelope.credential_kind != CodexCredentialKind::ChatgptOAuth
            || envelope.payload.access_token.trim().is_empty()
            || envelope.payload.refresh_token.trim().is_empty()
        {
            return Err(CodexAccountProfileError::MissingRequiredCredentialField);
        }
        Ok(envelope)
    }

    pub(crate) fn snapshot(&self) -> CodexCredentialSnapshot {
        CodexCredentialSnapshot {
            access_token: self.payload.access_token.clone(),
            account_id: self
                .payload
                .account_id
                .clone()
                .or_else(|| self.payload.id_token.chatgpt_account_id.clone()),
        }
    }

    fn serialized(&self) -> Result<String, CodexAccountProfileError> {
        let value = serde_json::to_string(self)
            .map_err(|_| CodexAccountProfileError::InvalidCredentialEnvelope)?;
        if value.len() > MAX_CREDENTIAL_ENVELOPE_BYTES {
            return Err(CodexAccountProfileError::CredentialEnvelopeTooLarge);
        }
        Ok(value)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexAccountProfileError {
    InvalidCodexAccountProfileId,
    DuplicateCodexAccountProfile,
    InvalidAccountState,
    DuplicateCredentialReference,
    TooManyProfiles,
    RegistryUnavailable,
    RegistryMalformed,
    RegistryTooLarge,
    UnsupportedSchemaVersion,
    AtomicWriteFailed,
    CredentialStoreRejected,
    UnknownCodexAccountProfile,
    UnsupportedStateTransition,
    InvalidCredentialEnvelope,
    UnsupportedCredentialEnvelopeVersion,
    CredentialEnvelopeTooLarge,
    MissingRequiredCredentialField,
}
impl fmt::Display for CodexAccountProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCodexAccountProfileId => "Codex account profile ID is invalid",
            Self::DuplicateCodexAccountProfile => "Codex account profile already exists",
            Self::InvalidAccountState => "Codex account profile state is invalid",
            Self::DuplicateCredentialReference => "Codex credential reference is already assigned",
            Self::TooManyProfiles => "too many Codex account profiles",
            Self::RegistryUnavailable => "Codex account registry is unavailable",
            Self::RegistryMalformed => "Codex account registry is malformed",
            Self::RegistryTooLarge => "Codex account registry is too large",
            Self::UnsupportedSchemaVersion => "Codex account registry schema is unsupported",
            Self::AtomicWriteFailed => "Codex account registry write failed",
            Self::CredentialStoreRejected => "Codex credential store rejected the operation",
            Self::UnknownCodexAccountProfile => "Codex account profile was not found",
            Self::UnsupportedStateTransition => "Codex account state transition is unsupported",
            Self::InvalidCredentialEnvelope => "Codex credential envelope is invalid",
            Self::UnsupportedCredentialEnvelopeVersion => {
                "Codex credential envelope version is unsupported"
            }
            Self::CredentialEnvelopeTooLarge => "Codex credential envelope is too large",
            Self::MissingRequiredCredentialField => {
                "Codex credential envelope is missing required fields"
            }
        })
    }
}

/// Stores the native Codex auth payload under the exact connection-derived credential reference.
/// The serialized payload crosses only the credential-store boundary and is never part of account
/// metadata, routing profiles, or diagnostics.
pub fn store_codex_auth(
    connection_id: &str,
    auth: &AuthDotJson,
) -> Result<String, CodexAccountProfileError> {
    let reference = CodexAccountProfileRegistry::credential_reference_for(connection_id)?;
    let payload = CodexCredentialEnvelope::from_auth(auth)?.serialized()?;
    let secret = super::provider_connection::CredentialSecret::new(payload)
        .map_err(|_| CodexAccountProfileError::CredentialStoreRejected)?;
    super::native_credential_store::NativeCredentialStore::new()
        .store(
            &super::provider_connection::CredentialReference::new(reference.clone())
                .map_err(|_| CodexAccountProfileError::CredentialStoreRejected)?,
            secret,
        )
        .map_err(|_| CodexAccountProfileError::CredentialStoreRejected)?;
    Ok(reference)
}

pub fn retrieve_codex_envelope(
    connection_id: &str,
) -> Result<CodexCredentialEnvelope, CodexAccountProfileError> {
    let reference = CodexAccountProfileRegistry::credential_reference_for(connection_id)?;
    let reference = super::provider_connection::CredentialReference::new(reference)
        .map_err(|_| CodexAccountProfileError::CredentialStoreRejected)?;
    let secret = super::native_credential_store::NativeCredentialStore::new()
        .retrieve(&reference)
        .map_err(|_| CodexAccountProfileError::CredentialStoreRejected)?;
    CodexCredentialEnvelope::parse(secret.expose_for_auth())
}

pub fn delete_codex_auth(connection_id: &str) -> Result<(), CodexAccountProfileError> {
    let reference = CodexAccountProfileRegistry::credential_reference_for(connection_id)?;
    let reference = super::provider_connection::CredentialReference::new(reference)
        .map_err(|_| CodexAccountProfileError::CredentialStoreRejected)?;
    super::native_credential_store::NativeCredentialStore::new()
        .delete(&reference)
        .map_err(|_| CodexAccountProfileError::CredentialStoreRejected)
}

pub fn codex_auth_exists(connection_id: &str) -> Result<bool, CodexAccountProfileError> {
    let reference = CodexAccountProfileRegistry::credential_reference_for(connection_id)?;
    let reference = super::provider_connection::CredentialReference::new(reference)
        .map_err(|_| CodexAccountProfileError::CredentialStoreRejected)?;
    super::native_credential_store::NativeCredentialStore::new()
        .contains(&reference)
        .map_err(|_| CodexAccountProfileError::CredentialStoreRejected)
}
impl std::error::Error for CodexAccountProfileError {}

#[cfg(test)]
#[path = "codex_account_profile_tests.rs"]
mod codex_account_profile_tests;
