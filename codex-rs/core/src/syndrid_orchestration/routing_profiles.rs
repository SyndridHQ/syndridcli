use super::omniroute::OmniRouteConnectionMetadata;
use super::omniroute::OmniRouteRegistry;
use super::omniroute::ProviderSelection;
use super::provider_connection::ConnectionValidationStatus;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::NamedTempFile;

const SCHEMA_VERSION: u32 = 1;
const MAX_FILE_BYTES: usize = 256 * 1024;
const MAX_PROFILES: usize = 32;
const MAX_ASSIGNMENTS: usize = 16;
const MAX_ID_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 256;
const MAX_DESCRIPTION_BYTES: usize = 512;
const MAX_MODEL_BYTES: usize = 256;

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RoutingProfileId(String);

impl RoutingProfileId {
    pub fn new(value: impl Into<String>) -> Result<Self, RoutingProfileError> {
        let value = value.into();
        validate_id(&value, RoutingProfileError::InvalidProfileId)?;
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RoutingProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("RoutingProfileId").field(&self.0).finish()
    }
}
impl fmt::Display for RoutingProfileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}
impl Serialize for RoutingProfileId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}
impl<'de> Deserialize<'de> for RoutingProfileId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingRole {
    Main,
    Planner,
    Executor,
    Verifier,
    Repair,
}

impl RoutingRole {
    pub fn required_for_sequential(self) -> bool {
        matches!(
            self,
            Self::Main | Self::Planner | Self::Executor | Self::Verifier
        )
    }
    pub fn parse(value: &str) -> Result<Self, RoutingProfileError> {
        match value {
            "main" => Ok(Self::Main),
            "planner" => Ok(Self::Planner),
            "executor" => Ok(Self::Executor),
            "verifier" => Ok(Self::Verifier),
            "repair" => Ok(Self::Repair),
            _ => Err(RoutingProfileError::InvalidRole),
        }
    }
}
impl fmt::Display for RoutingRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Main => "main",
            Self::Planner => "planner",
            Self::Executor => "executor",
            Self::Verifier => "verifier",
            Self::Repair => "repair",
        })
    }
}

#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutingAssignment {
    pub connection_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub enabled: bool,
    pub label: Option<String>,
}

impl fmt::Debug for RoutingAssignment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RoutingAssignment")
            .field("connection_id", &self.connection_id)
            .field("provider_id", &self.provider_id)
            .field("model_id", &self.model_id)
            .field("enabled", &self.enabled)
            .field("label", &self.label)
            .finish()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutingProfile {
    pub id: RoutingProfileId,
    pub name: String,
    pub assignments: BTreeMap<RoutingRole, RoutingAssignment>,
    pub created_at: u64,
    pub updated_at: u64,
    pub schema_version: u32,
    pub enabled: bool,
    pub description: Option<String>,
}

impl RoutingProfile {
    pub fn new(
        id: RoutingProfileId,
        name: impl Into<String>,
        now: u64,
    ) -> Result<Self, RoutingProfileError> {
        let name = bounded_text(
            name.into(),
            MAX_NAME_BYTES,
            RoutingProfileError::InvalidProfileName,
        )?;
        Ok(Self {
            id,
            name,
            assignments: BTreeMap::new(),
            created_at: now,
            updated_at: now,
            schema_version: SCHEMA_VERSION,
            enabled: true,
            description: None,
        })
    }
    pub fn assign(
        &mut self,
        role: RoutingRole,
        assignment: RoutingAssignment,
    ) -> Result<(), RoutingProfileError> {
        validate_assignment(&assignment)?;
        if self.assignments.contains_key(&role) {
            return Err(RoutingProfileError::DuplicateRoleAssignment);
        }
        if self.assignments.len() >= MAX_ASSIGNMENTS {
            return Err(RoutingProfileError::TooManyAssignments);
        }
        self.assignments.insert(role, assignment);
        Ok(())
    }
    pub fn replace_assignment(
        &mut self,
        role: RoutingRole,
        assignment: RoutingAssignment,
    ) -> Result<(), RoutingProfileError> {
        validate_assignment(&assignment)?;
        self.assignments.insert(role, assignment);
        Ok(())
    }
    pub fn unassign(&mut self, role: RoutingRole) -> Result<(), RoutingProfileError> {
        if self.assignments.remove(&role).is_none() {
            return Err(RoutingProfileError::MissingRoleAssignment);
        }
        Ok(())
    }
    pub fn validate_required_roles(&self) -> Result<(), RoutingProfileError> {
        for role in [
            RoutingRole::Main,
            RoutingRole::Planner,
            RoutingRole::Executor,
            RoutingRole::Verifier,
        ] {
            let assignment = self
                .assignments
                .get(&role)
                .ok_or(RoutingProfileError::MissingRoleAssignment)?;
            if !assignment.enabled {
                return Err(RoutingProfileError::DisabledAssignment);
            }
        }
        Ok(())
    }
    pub fn resolve_role(
        &self,
        role: RoutingRole,
    ) -> Result<ProviderSelection, RoutingProfileError> {
        let assignment = self
            .assignments
            .get(&role)
            .ok_or(RoutingProfileError::MissingRoleAssignment)?;
        if !assignment.enabled {
            return Err(RoutingProfileError::DisabledAssignment);
        }
        ProviderSelection::new(
            &assignment.connection_id,
            &assignment.provider_id,
            &assignment.model_id,
        )
        .map_err(|_| RoutingProfileError::InvalidAssignment)
    }

    pub fn resolve_required_sequential_selections(
        &self,
    ) -> Result<[ProviderSelection; 4], RoutingProfileError> {
        Ok([
            self.resolve_role(RoutingRole::Main)?,
            self.resolve_role(RoutingRole::Planner)?,
            self.resolve_role(RoutingRole::Executor)?,
            self.resolve_role(RoutingRole::Verifier)?,
        ])
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutingProfileRegistry {
    pub schema_version: u32,
    pub active_profile_id: Option<RoutingProfileId>,
    pub profiles: BTreeMap<RoutingProfileId, RoutingProfile>,
}

impl Default for RoutingProfileRegistry {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            active_profile_id: None,
            profiles: BTreeMap::new(),
        }
    }
}

impl RoutingProfileRegistry {
    pub fn load(path: &Path) -> Result<Self, RoutingProfileError> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(_) => return Err(RoutingProfileError::RegistryUnavailable),
        };
        if bytes.len() > MAX_FILE_BYTES {
            return Err(RoutingProfileError::RegistryTooLarge);
        }
        let registry: Self =
            serde_json::from_slice(&bytes).map_err(|_| RoutingProfileError::RegistryMalformed)?;
        registry.validate()?;
        Ok(registry)
    }
    pub fn save(&self, path: &Path) -> Result<(), RoutingProfileError> {
        self.validate()?;
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|_| RoutingProfileError::RegistryMalformed)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(RoutingProfileError::RegistryTooLarge);
        }
        let parent = path
            .parent()
            .ok_or(RoutingProfileError::AtomicWriteFailed)?;
        std::fs::create_dir_all(parent).map_err(|_| RoutingProfileError::AtomicWriteFailed)?;
        let temporary =
            NamedTempFile::new_in(parent).map_err(|_| RoutingProfileError::AtomicWriteFailed)?;
        std::fs::write(temporary.path(), bytes)
            .map_err(|_| RoutingProfileError::AtomicWriteFailed)?;
        temporary
            .persist(path)
            .map_err(|_| RoutingProfileError::AtomicWriteFailed)?;
        Ok(())
    }
    pub fn insert(&mut self, profile: RoutingProfile) -> Result<(), RoutingProfileError> {
        validate_profile_structure(&profile)?;
        if self.profiles.len() >= MAX_PROFILES {
            return Err(RoutingProfileError::TooManyProfiles);
        }
        if self.profiles.contains_key(&profile.id) {
            return Err(RoutingProfileError::DuplicateProfileId);
        }
        self.profiles.insert(profile.id.clone(), profile);
        Ok(())
    }
    pub fn get(&self, id: &RoutingProfileId) -> Option<&RoutingProfile> {
        self.profiles.get(id)
    }
    pub fn get_mut(&mut self, id: &RoutingProfileId) -> Option<&mut RoutingProfile> {
        self.profiles.get_mut(id)
    }
    pub fn profiles(&self) -> impl Iterator<Item = &RoutingProfile> {
        self.profiles.values()
    }
    pub fn activate(
        &mut self,
        id: &RoutingProfileId,
    ) -> Result<(Option<RoutingProfileId>, RoutingProfileId), RoutingProfileError> {
        let profile = self
            .profiles
            .get(id)
            .ok_or(RoutingProfileError::UnknownProfile)?;
        if !profile.enabled {
            return Err(RoutingProfileError::DisabledProfile);
        }
        profile.validate_required_roles()?;
        let previous = self.active_profile_id.replace(id.clone());
        Ok((previous, id.clone()))
    }
    pub fn active(&self) -> Result<&RoutingProfile, RoutingProfileError> {
        self.active_profile_id
            .as_ref()
            .and_then(|id| self.profiles.get(id))
            .ok_or(RoutingProfileError::MissingActiveProfile)
    }
    pub fn delete(&mut self, id: &RoutingProfileId) -> Result<(), RoutingProfileError> {
        if self.active_profile_id.as_ref() == Some(id) {
            return Err(RoutingProfileError::ActiveProfileDeletionRejected);
        }
        self.profiles
            .remove(id)
            .map(|_| ())
            .ok_or(RoutingProfileError::UnknownProfile)
    }
    fn validate(&self) -> Result<(), RoutingProfileError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(RoutingProfileError::UnsupportedSchemaVersion);
        }
        if self.profiles.len() > MAX_PROFILES {
            return Err(RoutingProfileError::TooManyProfiles);
        }
        if let Some(active) = &self.active_profile_id {
            if !self.profiles.contains_key(active) {
                return Err(RoutingProfileError::MissingActiveProfile);
            }
        }
        for profile in self.profiles.values() {
            validate_profile_structure(profile)?;
        }
        Ok(())
    }
}

pub struct RoutingProfileStore {
    path: PathBuf,
    write_lock: Arc<Mutex<()>>,
}
impl RoutingProfileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Arc::new(Mutex::new(())),
        }
    }
    pub fn load(&self) -> Result<RoutingProfileRegistry, RoutingProfileError> {
        RoutingProfileRegistry::load(&self.path)
    }
    pub fn save(&self, registry: &RoutingProfileRegistry) -> Result<(), RoutingProfileError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| RoutingProfileError::AtomicWriteFailed)?;
        registry.save(&self.path)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoutingConnectionInfo {
    pub connection_id: String,
    pub provider_id: String,
    pub enabled: bool,
    pub validation: ConnectionValidationStatus,
    pub authentication_supported: bool,
    pub models: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RoutingConnectionDirectory {
    connections: BTreeMap<String, RoutingConnectionInfo>,
}
impl RoutingConnectionDirectory {
    pub fn insert(&mut self, info: RoutingConnectionInfo) {
        self.connections.insert(info.connection_id.clone(), info);
    }
    pub fn from_omniroute(registry: &OmniRouteRegistry) -> Self {
        let mut directory = Self::default();
        for connection in registry.connections() {
            directory.insert(connection_info(connection));
        }
        directory
    }
    pub fn validate_assignment(
        &self,
        assignment: &RoutingAssignment,
    ) -> Result<RoutingResolutionStatus, RoutingProfileError> {
        validate_assignment(assignment)?;
        let connection = self
            .connections
            .get(&assignment.connection_id)
            .ok_or(RoutingProfileError::UnknownConnection)?;
        if !connection.enabled {
            return Err(RoutingProfileError::DisabledConnection);
        }
        if connection.validation != ConnectionValidationStatus::Valid {
            return Err(RoutingProfileError::UnvalidatedConnection);
        }
        if connection.provider_id != assignment.provider_id {
            return Err(RoutingProfileError::ProviderMismatch);
        }
        if !connection.authentication_supported {
            return Err(RoutingProfileError::UnsupportedAuthenticationMethod);
        }
        match &connection.models {
            Some(models) if !models.iter().any(|model| model == &assignment.model_id) => {
                Err(RoutingProfileError::ModelNotFound)
            }
            Some(_) => Ok(RoutingResolutionStatus::LocallyValid),
            None => Ok(RoutingResolutionStatus::ModelUnverified),
        }
    }
    pub fn provider_id_for(&self, connection_id: &str) -> Option<&str> {
        self.connections
            .get(connection_id)
            .map(|connection| connection.provider_id.as_str())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingResolutionStatus {
    LocallyValid,
    ModelUnverified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RoutingProfileError {
    InvalidProfileId,
    InvalidProfileName,
    DuplicateProfileId,
    UnknownProfile,
    DisabledProfile,
    ActiveProfileDeletionRejected,
    MissingActiveProfile,
    MissingRoleAssignment,
    DuplicateRoleAssignment,
    DisabledAssignment,
    InvalidAssignment,
    InvalidRole,
    UnknownConnection,
    DisabledConnection,
    UnvalidatedConnection,
    ProviderMismatch,
    InvalidModelId,
    ModelUnverified,
    ModelNotFound,
    UnsupportedProvider,
    UnsupportedAuthenticationMethod,
    RegistryUnavailable,
    RegistryMalformed,
    RegistryTooLarge,
    TooManyProfiles,
    TooManyAssignments,
    AtomicWriteFailed,
    UnsupportedSchemaVersion,
}
impl fmt::Display for RoutingProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidProfileId => "routing profile ID is invalid",
            Self::InvalidProfileName => "routing profile name is invalid",
            Self::DuplicateProfileId => "routing profile ID already exists",
            Self::UnknownProfile => "routing profile was not found",
            Self::DisabledProfile => "routing profile is disabled",
            Self::ActiveProfileDeletionRejected => "active routing profile cannot be deleted",
            Self::MissingActiveProfile => "no active routing profile is configured",
            Self::MissingRoleAssignment => "routing role has no assignment",
            Self::DuplicateRoleAssignment => "routing role is already assigned",
            Self::DisabledAssignment => "routing assignment is disabled",
            Self::InvalidAssignment => "routing assignment is invalid",
            Self::InvalidRole => "routing role is invalid",
            Self::UnknownConnection => "provider connection was not found",
            Self::DisabledConnection => "provider connection is disabled",
            Self::UnvalidatedConnection => "provider connection is not validated",
            Self::ProviderMismatch => "provider ID does not match the connection",
            Self::InvalidModelId => "model ID is invalid",
            Self::ModelUnverified => "model could not be verified",
            Self::ModelNotFound => "model was not found in the authoritative catalog",
            Self::UnsupportedProvider => "provider is unsupported",
            Self::UnsupportedAuthenticationMethod => {
                "provider authentication method is unsupported"
            }
            Self::RegistryUnavailable => "routing profile registry is unavailable",
            Self::RegistryMalformed => "routing profile registry is malformed",
            Self::RegistryTooLarge => "routing profile registry is too large",
            Self::TooManyProfiles => "routing profile registry has too many profiles",
            Self::TooManyAssignments => "routing profile has too many assignments",
            Self::AtomicWriteFailed => "routing profile registry could not be written atomically",
            Self::UnsupportedSchemaVersion => "routing profile schema version is unsupported",
        })
    }
}
impl std::error::Error for RoutingProfileError {}

fn validate_id(value: &str, error: RoutingProfileError) -> Result<(), RoutingProfileError> {
    if value.is_empty()
        || value.len() > MAX_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(error);
    }
    Ok(())
}
fn bounded_text(
    value: String,
    max: usize,
    error: RoutingProfileError,
) -> Result<String, RoutingProfileError> {
    if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(error);
    }
    Ok(value)
}
fn validate_assignment(assignment: &RoutingAssignment) -> Result<(), RoutingProfileError> {
    if assignment.connection_id.trim().is_empty() || assignment.provider_id.trim().is_empty() {
        return Err(RoutingProfileError::InvalidAssignment);
    }
    if assignment.model_id.trim().is_empty() {
        return Err(RoutingProfileError::InvalidModelId);
    }
    if assignment.model_id.len() > MAX_MODEL_BYTES {
        return Err(RoutingProfileError::InvalidModelId);
    }
    if let Some(label) = &assignment.label {
        bounded_text(
            label.clone(),
            MAX_NAME_BYTES,
            RoutingProfileError::InvalidAssignment,
        )?;
    }
    Ok(())
}
fn validate_profile_structure(profile: &RoutingProfile) -> Result<(), RoutingProfileError> {
    if profile.schema_version != SCHEMA_VERSION {
        return Err(RoutingProfileError::UnsupportedSchemaVersion);
    }
    if profile.assignments.len() > MAX_ASSIGNMENTS {
        return Err(RoutingProfileError::TooManyAssignments);
    }
    bounded_text(
        profile.name.clone(),
        MAX_NAME_BYTES,
        RoutingProfileError::InvalidProfileName,
    )?;
    if let Some(description) = &profile.description {
        bounded_text(
            description.clone(),
            MAX_DESCRIPTION_BYTES,
            RoutingProfileError::InvalidProfileName,
        )?;
    }
    for assignment in profile.assignments.values() {
        validate_assignment(assignment)?;
    }
    Ok(())
}
fn connection_info(connection: &OmniRouteConnectionMetadata) -> RoutingConnectionInfo {
    RoutingConnectionInfo {
        connection_id: connection.connection_id.clone(),
        provider_id: connection.provider_id.clone(),
        enabled: connection.enabled,
        validation: connection.validation.status,
        authentication_supported: true,
        models: Some(connection.models.clone()),
    }
}

#[cfg(test)]
#[path = "routing_profile_tests.rs"]
mod routing_profile_tests;
