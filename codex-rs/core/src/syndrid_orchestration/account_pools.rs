//! Canonical, explicit-only named pools of provider identities.
//!
//! Pool definitions are inert configuration. This module never authenticates, contacts a
//! provider, rotates members, or substitutes a member when the explicitly selected member is
//! unavailable.

use super::codex_accounts::CodexAccountProfileId;
use super::codex_accounts::CodexAccountProfileRegistry;
use super::codex_accounts::CodexAccountProfileState;
use super::omniroute::OmniRouteRegistry;
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

pub const ACCOUNT_POOL_FILE: &str = "syndrid-account-pools.json";
pub const MAX_ACCOUNT_POOL_FILE_BYTES: usize = 256 * 1024;
const SCHEMA_VERSION: u32 = 1;
const MAX_POOLS: usize = 32;
const MAX_MEMBERS_PER_POOL: usize = 32;
const MAX_POOL_ID_BYTES: usize = 128;
const MAX_MEMBER_ID_BYTES: usize = 128;
const MAX_DISPLAY_NAME_BYTES: usize = 256;

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PoolId(String);

impl PoolId {
    pub fn new(value: impl Into<String>) -> Result<Self, AccountPoolError> {
        Self::bounded(
            value.into(),
            MAX_POOL_ID_BYTES,
            AccountPoolError::InvalidPoolId,
        )
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn bounded(
        value: String,
        max: usize,
        error: AccountPoolError,
    ) -> Result<Self, AccountPoolError> {
        if value.trim().is_empty()
            || value.len() > max
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(error);
        }
        Ok(Self(value))
    }
}

impl fmt::Debug for PoolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PoolId").field(&self.0).finish()
    }
}

impl fmt::Display for PoolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PoolMemberId(String);

impl PoolMemberId {
    pub fn new(value: impl Into<String>) -> Result<Self, AccountPoolError> {
        PoolId::bounded(
            value.into(),
            MAX_MEMBER_ID_BYTES,
            AccountPoolError::InvalidMemberId,
        )
        .map(|id| Self(id.0))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PoolMemberId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("PoolMemberId").field(&self.0).finish()
    }
}

impl fmt::Display for PoolMemberId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountPoolProviderFamily {
    NativeCodex,
    OmniRoute,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountPoolTarget {
    NativeCodexAccount(CodexAccountProfileId),
    OmniRouteConnection(String),
}

impl AccountPoolTarget {
    pub fn native_codex(account_profile_id: CodexAccountProfileId) -> Self {
        Self::NativeCodexAccount(account_profile_id)
    }

    pub fn omniroute(connection_id: impl Into<String>) -> Result<Self, AccountPoolError> {
        let connection_id = connection_id.into();
        if connection_id.trim().is_empty() || connection_id.len() > MAX_MEMBER_ID_BYTES {
            return Err(AccountPoolError::InvalidConnectionId);
        }
        Ok(Self::OmniRouteConnection(connection_id))
    }

    pub fn provider_family(&self) -> AccountPoolProviderFamily {
        match self {
            Self::NativeCodexAccount(_) => AccountPoolProviderFamily::NativeCodex,
            Self::OmniRouteConnection(_) => AccountPoolProviderFamily::OmniRoute,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountPoolMember {
    pub id: PoolMemberId,
    pub target: AccountPoolTarget,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AccountPoolSelectionPolicy {
    ExplicitMember(PoolMemberId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedAccountPool {
    pub id: PoolId,
    pub display_name: String,
    pub provider_family: AccountPoolProviderFamily,
    pub members: Vec<AccountPoolMember>,
    pub selection_policy: AccountPoolSelectionPolicy,
}

impl NamedAccountPool {
    pub fn validate_structure(&self) -> Result<(), AccountPoolError> {
        if self.display_name.trim().is_empty() || self.display_name.len() > MAX_DISPLAY_NAME_BYTES {
            return Err(AccountPoolError::InvalidDisplayName);
        }
        if self.members.is_empty() {
            return Err(AccountPoolError::EmptyPool);
        }
        if self.members.len() > MAX_MEMBERS_PER_POOL {
            return Err(AccountPoolError::TooManyMembers);
        }
        let mut ids = BTreeMap::new();
        for member in &self.members {
            if ids.insert(&member.id, ()).is_some() {
                return Err(AccountPoolError::DuplicateMemberId);
            }
            if member.target.provider_family() != self.provider_family {
                return Err(AccountPoolError::ProviderFamilyMismatch);
            }
        }
        let AccountPoolSelectionPolicy::ExplicitMember(selected) = &self.selection_policy;
        if !self.members.iter().any(|member| member.id == *selected) {
            return Err(AccountPoolError::SelectedMemberNotInPool);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct NamedAccountPoolRegistry {
    pools: BTreeMap<PoolId, NamedAccountPool>,
}

impl NamedAccountPoolRegistry {
    pub fn remove(&mut self, id: &PoolId) -> Option<NamedAccountPool> {
        self.pools.remove(id)
    }

    pub fn insert(&mut self, pool: NamedAccountPool) -> Result<(), AccountPoolError> {
        pool.validate_structure()?;
        if self.pools.len() >= MAX_POOLS {
            return Err(AccountPoolError::TooManyPools);
        }
        if self.pools.contains_key(&pool.id) {
            return Err(AccountPoolError::DuplicatePoolId);
        }
        self.pools.insert(pool.id.clone(), pool);
        Ok(())
    }

    pub fn get(&self, id: &PoolId) -> Option<&NamedAccountPool> {
        self.pools.get(id)
    }

    pub fn pools(&self) -> impl Iterator<Item = &NamedAccountPool> {
        self.pools.values()
    }

    pub fn validate_structure(&self) -> Result<(), AccountPoolError> {
        if self.pools.len() > MAX_POOLS {
            return Err(AccountPoolError::TooManyPools);
        }
        for pool in self.pools.values() {
            pool.validate_structure()?;
        }
        Ok(())
    }

    pub fn resolve_pool(
        &self,
        pool_id: &PoolId,
        accounts: &CodexAccountProfileRegistry,
        connections: &OmniRouteRegistry,
    ) -> Result<ResolvedPoolMember, PoolResolutionError> {
        let pool = self
            .pools
            .get(pool_id)
            .ok_or(PoolResolutionError::PoolNotFound)?;
        pool.validate_structure()
            .map_err(PoolResolutionError::InvalidPool)?;
        let AccountPoolSelectionPolicy::ExplicitMember(selected_id) = &pool.selection_policy;
        let member = pool
            .members
            .iter()
            .find(|member| member.id == *selected_id)
            .ok_or(PoolResolutionError::SelectedMemberNotInPool)?;
        match &member.target {
            AccountPoolTarget::NativeCodexAccount(profile_id) => {
                let profile = accounts
                    .get(profile_id)
                    .ok_or(PoolResolutionError::MissingAccountReference)?;
                if profile.state != CodexAccountProfileState::Connected
                    || !profile.enabled
                    || profile.validation != ConnectionValidationStatus::Valid
                {
                    return Err(PoolResolutionError::UnavailableAccountReference);
                }
                Ok(ResolvedPoolMember {
                    pool_id: pool.id.clone(),
                    member_id: member.id.clone(),
                    target: member.target.clone(),
                })
            }
            AccountPoolTarget::OmniRouteConnection(connection_id) => {
                let connection = connections
                    .get(connection_id)
                    .ok_or(PoolResolutionError::MissingConnectionReference)?;
                if connection.provider_id != super::omniroute::OMNIROUTE_PROVIDER_ID
                    || !connection.enabled
                    || connection.validation.status != ConnectionValidationStatus::Valid
                {
                    return Err(PoolResolutionError::UnavailableConnectionReference);
                }
                Ok(ResolvedPoolMember {
                    pool_id: pool.id.clone(),
                    member_id: member.id.clone(),
                    target: member.target.clone(),
                })
            }
        }
    }

    pub fn readiness(
        &self,
        accounts: &CodexAccountProfileRegistry,
        connections: &OmniRouteRegistry,
    ) -> BTreeMap<PoolId, PoolReadiness> {
        self.pools
            .values()
            .map(|pool| {
                let readiness = match self.resolve_pool(&pool.id, accounts, connections) {
                    Ok(_) => PoolReadiness::Ready,
                    Err(PoolResolutionError::MissingAccountReference) => {
                        PoolReadiness::MissingAccountReference
                    }
                    Err(PoolResolutionError::MissingConnectionReference) => {
                        PoolReadiness::MissingConnectionReference
                    }
                    Err(PoolResolutionError::UnavailableAccountReference) => {
                        PoolReadiness::UnavailableAccountReference
                    }
                    Err(PoolResolutionError::UnavailableConnectionReference) => {
                        PoolReadiness::UnavailableConnectionReference
                    }
                    Err(PoolResolutionError::InvalidPool(_)) => PoolReadiness::InvalidStructure,
                    Err(
                        PoolResolutionError::PoolNotFound
                        | PoolResolutionError::SelectedMemberNotInPool,
                    ) => PoolReadiness::InvalidStructure,
                };
                (pool.id.clone(), readiness)
            })
            .collect()
    }

    /// Reports readiness for every preserved member without changing the selected member.
    pub fn member_readiness(
        &self,
        pool_id: &PoolId,
        accounts: &CodexAccountProfileRegistry,
        connections: &OmniRouteRegistry,
    ) -> Result<BTreeMap<PoolMemberId, PoolMemberReadiness>, PoolResolutionError> {
        let pool = self
            .pools
            .get(pool_id)
            .ok_or(PoolResolutionError::PoolNotFound)?;
        pool.validate_structure()
            .map_err(PoolResolutionError::InvalidPool)?;
        Ok(pool
            .members
            .iter()
            .map(|member| {
                (
                    member.id.clone(),
                    member_readiness(&member.target, accounts, connections),
                )
            })
            .collect())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedPoolMember {
    pub pool_id: PoolId,
    pub member_id: PoolMemberId,
    pub target: AccountPoolTarget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolReadiness {
    Ready,
    InvalidStructure,
    MissingAccountReference,
    UnavailableAccountReference,
    MissingConnectionReference,
    UnavailableConnectionReference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolMemberReadiness {
    Ready,
    MissingAccountReference,
    UnavailableAccountReference,
    MissingConnectionReference,
    UnavailableConnectionReference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolResolutionError {
    PoolNotFound,
    SelectedMemberNotInPool,
    MissingAccountReference,
    UnavailableAccountReference,
    MissingConnectionReference,
    UnavailableConnectionReference,
    InvalidPool(AccountPoolError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountPoolError {
    InvalidPoolId,
    InvalidMemberId,
    InvalidDisplayName,
    InvalidAccountProfileId,
    InvalidConnectionId,
    DuplicatePoolId,
    DuplicateMemberId,
    EmptyPool,
    TooManyPools,
    TooManyMembers,
    ProviderFamilyMismatch,
    SelectedMemberNotInPool,
    RegistryUnavailable,
    RegistryMalformed,
    RegistryTooLarge,
    UnsupportedSchemaVersion,
    AtomicWriteFailed,
}

impl fmt::Display for AccountPoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPoolId => "account pool ID is invalid",
            Self::InvalidMemberId => "account pool member ID is invalid",
            Self::InvalidDisplayName => "account pool display name is invalid",
            Self::InvalidAccountProfileId => "Codex account profile ID is invalid",
            Self::InvalidConnectionId => "OmniRoute connection ID is invalid",
            Self::DuplicatePoolId => "account pool ID already exists",
            Self::DuplicateMemberId => "account pool member ID already exists",
            Self::EmptyPool => "account pool must contain a member",
            Self::TooManyPools => "account pool registry has too many pools",
            Self::TooManyMembers => "account pool has too many members",
            Self::ProviderFamilyMismatch => "account pool member provider family does not match",
            Self::SelectedMemberNotInPool => "selected account pool member is not in the pool",
            Self::RegistryUnavailable => "account pool registry is unavailable",
            Self::RegistryMalformed => "account pool registry is malformed",
            Self::RegistryTooLarge => "account pool registry is too large",
            Self::UnsupportedSchemaVersion => "account pool schema is unsupported",
            Self::AtomicWriteFailed => "account pool registry write failed",
        })
    }
}

impl std::error::Error for AccountPoolError {}

fn member_readiness(
    target: &AccountPoolTarget,
    accounts: &CodexAccountProfileRegistry,
    connections: &OmniRouteRegistry,
) -> PoolMemberReadiness {
    match target {
        AccountPoolTarget::NativeCodexAccount(profile_id) => {
            let Some(profile) = accounts.get(profile_id) else {
                return PoolMemberReadiness::MissingAccountReference;
            };
            if profile.state == CodexAccountProfileState::Connected
                && profile.enabled
                && profile.validation == ConnectionValidationStatus::Valid
            {
                PoolMemberReadiness::Ready
            } else {
                PoolMemberReadiness::UnavailableAccountReference
            }
        }
        AccountPoolTarget::OmniRouteConnection(connection_id) => {
            let Some(connection) = connections.get(connection_id) else {
                return PoolMemberReadiness::MissingConnectionReference;
            };
            if connection.provider_id == super::omniroute::OMNIROUTE_PROVIDER_ID
                && connection.enabled
                && connection.validation.status == ConnectionValidationStatus::Valid
            {
                PoolMemberReadiness::Ready
            } else {
                PoolMemberReadiness::UnavailableConnectionReference
            }
        }
    }
}

#[derive(Clone)]
pub struct NamedAccountPoolStore {
    path: PathBuf,
    write_lock: Arc<Mutex<()>>,
}

impl NamedAccountPoolStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<NamedAccountPoolRegistry, AccountPoolError> {
        NamedAccountPoolRegistry::load(&self.path)
    }

    pub fn save(&self, registry: &NamedAccountPoolRegistry) -> Result<(), AccountPoolError> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|_| AccountPoolError::AtomicWriteFailed)?;
        registry.save(&self.path)
    }
}

impl NamedAccountPoolRegistry {
    pub fn load(path: &Path) -> Result<Self, AccountPoolError> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err(AccountPoolError::RegistryUnavailable),
        };
        if bytes.len() > MAX_ACCOUNT_POOL_FILE_BYTES {
            return Err(AccountPoolError::RegistryTooLarge);
        }
        let dto: RegistryDto =
            serde_json::from_slice(&bytes).map_err(|_| AccountPoolError::RegistryMalformed)?;
        if dto.schema_version != SCHEMA_VERSION {
            return Err(AccountPoolError::UnsupportedSchemaVersion);
        }
        if dto.pools.len() > MAX_POOLS {
            return Err(AccountPoolError::TooManyPools);
        }
        let mut registry = Self::default();
        for pool in dto.pools {
            registry.insert(pool.try_into()?)?;
        }
        Ok(registry)
    }

    pub fn save(&self, path: &Path) -> Result<(), AccountPoolError> {
        self.validate_structure()?;
        let dto = RegistryDto::from(self);
        let bytes =
            serde_json::to_vec_pretty(&dto).map_err(|_| AccountPoolError::RegistryMalformed)?;
        if bytes.len() > MAX_ACCOUNT_POOL_FILE_BYTES {
            return Err(AccountPoolError::RegistryTooLarge);
        }
        let parent = path.parent().ok_or(AccountPoolError::AtomicWriteFailed)?;
        std::fs::create_dir_all(parent).map_err(|_| AccountPoolError::AtomicWriteFailed)?;
        let mut temporary =
            NamedTempFile::new_in(parent).map_err(|_| AccountPoolError::AtomicWriteFailed)?;
        std::io::Write::write_all(&mut temporary, &bytes)
            .map_err(|_| AccountPoolError::AtomicWriteFailed)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| AccountPoolError::AtomicWriteFailed)?;
        temporary
            .persist(path)
            .map_err(|_| AccountPoolError::AtomicWriteFailed)?;
        Ok(())
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RegistryDto {
    schema_version: u32,
    pools: Vec<PoolDto>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PoolDto {
    id: String,
    display_name: String,
    provider_family: ProviderFamilyDto,
    members: Vec<MemberDto>,
    selection_policy: SelectionPolicyDto,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProviderFamilyDto {
    NativeCodex,
    OmniRoute,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct MemberDto {
    id: String,
    target: TargetDto,
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TargetDto {
    NativeCodexAccount { account_profile_id: String },
    OmniRouteConnection { connection_id: String },
}

#[derive(Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum SelectionPolicyDto {
    ExplicitMember { member_id: String },
}

impl TryFrom<PoolDto> for NamedAccountPool {
    type Error = AccountPoolError;
    fn try_from(dto: PoolDto) -> Result<Self, Self::Error> {
        let id = PoolId::new(dto.id)?;
        let display_name = dto.display_name;
        let provider_family = match dto.provider_family {
            ProviderFamilyDto::NativeCodex => AccountPoolProviderFamily::NativeCodex,
            ProviderFamilyDto::OmniRoute => AccountPoolProviderFamily::OmniRoute,
        };
        let members = dto
            .members
            .into_iter()
            .map(|member| {
                Ok(AccountPoolMember {
                    id: PoolMemberId::new(member.id)?,
                    target: match member.target {
                        TargetDto::NativeCodexAccount { account_profile_id } => {
                            AccountPoolTarget::NativeCodexAccount(
                                CodexAccountProfileId::new(account_profile_id)
                                    .map_err(|_| AccountPoolError::InvalidAccountProfileId)?,
                            )
                        }
                        TargetDto::OmniRouteConnection { connection_id } => {
                            AccountPoolTarget::omniroute(connection_id)?
                        }
                    },
                })
            })
            .collect::<Result<Vec<_>, AccountPoolError>>()?;
        let selection_policy = match dto.selection_policy {
            SelectionPolicyDto::ExplicitMember { member_id } => {
                AccountPoolSelectionPolicy::ExplicitMember(PoolMemberId::new(member_id)?)
            }
        };
        let pool = Self {
            id,
            display_name,
            provider_family,
            members,
            selection_policy,
        };
        pool.validate_structure()?;
        Ok(pool)
    }
}

impl From<&NamedAccountPoolRegistry> for RegistryDto {
    fn from(registry: &NamedAccountPoolRegistry) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            pools: registry.pools.values().map(PoolDto::from).collect(),
        }
    }
}

impl From<&NamedAccountPool> for PoolDto {
    fn from(pool: &NamedAccountPool) -> Self {
        Self {
            id: pool.id.0.clone(),
            display_name: pool.display_name.clone(),
            provider_family: match pool.provider_family {
                AccountPoolProviderFamily::NativeCodex => ProviderFamilyDto::NativeCodex,
                AccountPoolProviderFamily::OmniRoute => ProviderFamilyDto::OmniRoute,
            },
            members: pool.members.iter().map(MemberDto::from).collect(),
            selection_policy: match &pool.selection_policy {
                AccountPoolSelectionPolicy::ExplicitMember(member_id) => {
                    SelectionPolicyDto::ExplicitMember {
                        member_id: member_id.0.clone(),
                    }
                }
            },
        }
    }
}

impl From<&AccountPoolMember> for MemberDto {
    fn from(member: &AccountPoolMember) -> Self {
        Self {
            id: member.id.0.clone(),
            target: (&member.target).into(),
        }
    }
}

impl From<&AccountPoolTarget> for TargetDto {
    fn from(target: &AccountPoolTarget) -> Self {
        match target {
            AccountPoolTarget::NativeCodexAccount(id) => Self::NativeCodexAccount {
                account_profile_id: id.as_str().to_string(),
            },
            AccountPoolTarget::OmniRouteConnection(id) => Self::OmniRouteConnection {
                connection_id: id.clone(),
            },
        }
    }
}

#[cfg(test)]
#[path = "account_pools_tests.rs"]
mod account_pools_tests;
