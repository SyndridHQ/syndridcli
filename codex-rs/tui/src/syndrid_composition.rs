//! Trusted, session-scoped Syndrid composition owned by the TUI.
//!
//! This module supplies product-owned authority implementations to the neutral
//! app-server-client seam. It assembles inert runtime state for a future turn;
//! it does not select the production turn path or invoke a runner.

use crate::pool_authority::TuiPoolAuthority;
use crate::provider_setup::ProviderSetupSnapshot;
use codex_app_server_client::InProcessServerEvent;
use codex_app_server_client::ProductionExecutionCapability;
use codex_app_server_client::ProductionSessionRuntime;
use codex_app_server_client::ProductionTurnAdmissionInput;
use codex_app_server_client::ProductionTurnContextProvider;
use codex_app_server_client::ProductionTurnPreparationError;
use codex_app_server_client::TrustedApprovedToolAuthority;
use codex_app_server_client::TrustedApprovedToolSnapshot;
use codex_app_server_client::TrustedCompositionSnapshotError;
use codex_app_server_client::TrustedCompositionSnapshotRequest;
use codex_app_server_client::TrustedProductionProviderAuthority;
use codex_app_server_client::TrustedRoutingAuthority;
use codex_app_server_client::TrustedRoutingSnapshot;
use codex_app_server_client::TrustedSyndridCompositionDependencies;
use codex_app_server_client::TrustedSyndridCompositionSource;
use codex_app_server_client::assemble_trusted_production_runtime;
use codex_app_server_client::legacy_core::AccountPoolProviderFamily;
use codex_app_server_client::legacy_core::AccountPoolSelectionPolicy;
use codex_app_server_client::legacy_core::CodexAccountProfileRegistry;
use codex_app_server_client::legacy_core::CodexAccountProfileState;
use codex_app_server_client::legacy_core::ConnectionValidationStatus;
use codex_app_server_client::legacy_core::ExecutionModeSelection;
use codex_app_server_client::legacy_core::NamedAccountPoolRegistry;
use codex_app_server_client::legacy_core::OmniRouteRegistry;
use codex_app_server_client::legacy_core::OrchestrationMode;
use codex_app_server_client::legacy_core::ProductionProviderConstructionSnapshot;
use codex_app_server_client::legacy_core::ProductionProviderRoute;
use codex_app_server_client::legacy_core::ProductionRoundRobinProviderBinding;
use codex_app_server_client::legacy_core::ProviderConstructionError;
use codex_app_server_client::legacy_core::ProviderSelection;
use codex_app_server_client::legacy_core::ResolvedExecutionPolicy;
use codex_app_server_client::legacy_core::RoleCapabilityConfigError;
use codex_app_server_client::legacy_core::RoleCapabilityValidationContext;
use codex_app_server_client::legacy_core::RoutingConnectionDirectory;
use codex_app_server_client::legacy_core::RoutingProfile;
use codex_app_server_client::legacy_core::RoutingProfileRegistry;
use codex_app_server_client::legacy_core::RoutingProfileStore;
use codex_app_server_client::legacy_core::SessionExecutionPolicyState;
use codex_app_server_client::legacy_core::ValidatedRoleCapabilitySet;
use codex_app_server_client::legacy_core::load_role_capabilities;
use codex_app_server_client::legacy_core::native_codex_binding;
use codex_app_server_client::legacy_core::omniroute_binding;
use codex_app_server_client::legacy_core::openrouter_binding;
use codex_app_server_client::legacy_core::resolve_routing_profile;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::io::ErrorKind;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::RwLock;
use tokio::sync::mpsc;

const ROUTING_PROFILE_FILE: &str = "syndrid-routing-profiles.json";
const PROVIDER_CONNECTION_FILE: &str = "syndrid-provider-connections.json";
const CODEX_ACCOUNT_FILE: &str = "syndrid-codex-accounts.json";
const MAX_CONTEXT_BYTES: usize = 32 * 1024;
const MAX_CONTEXT_ENTRIES: usize = 32;
const MAX_CONTEXT_ENTRY_BYTES: usize = 8 * 1024;

#[derive(Clone, Debug)]
struct ContextEntry {
    role: &'static str,
    text: String,
}

#[derive(Default)]
struct ContextState {
    entries: VecDeque<ContextEntry>,
}

/// Session-owned bounded conversation context shared by ChatWidget and the
/// trusted composition source.
#[derive(Clone)]
pub(crate) struct TuiProductionContextProvider {
    state: Arc<RwLock<ContextState>>,
}

impl fmt::Debug for TuiProductionContextProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TuiProductionContextProvider")
            .field("state", &"<context-provider>")
            .finish()
    }
}

impl TuiProductionContextProvider {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(ContextState::default())),
        }
    }

    pub(crate) fn record_user_message(&self, text: &str) {
        self.record("user", text);
    }

    pub(crate) fn record_assistant_message(&self, text: &str) {
        self.record("assistant", text);
    }

    fn record(&self, role: &'static str, text: &str) {
        let text = truncate_utf8(text, MAX_CONTEXT_ENTRY_BYTES);
        if text.trim().is_empty() {
            return;
        }
        let Ok(mut state) = self.state.write() else {
            return;
        };
        if state.entries.len() == MAX_CONTEXT_ENTRIES {
            state.entries.pop_front();
        }
        state.entries.push_back(ContextEntry { role, text });
    }
}

impl ProductionTurnContextProvider for TuiProductionContextProvider {
    fn capture(
        &self,
        input: &ProductionTurnAdmissionInput,
    ) -> Result<Option<String>, ProductionTurnPreparationError> {
        let entries = self
            .state
            .read()
            .map_err(|_| ProductionTurnPreparationError::ContextUnavailable)?
            .entries
            .iter()
            .filter(|entry| entry.text != input.objective())
            .map(|entry| format!("{role}: {text}", role = entry.role, text = entry.text))
            .collect::<Vec<_>>();
        let mut context = String::new();
        for entry in entries {
            let separator = if context.is_empty() { "" } else { "\n" };
            let remaining = MAX_CONTEXT_BYTES.saturating_sub(context.len() + separator.len());
            if remaining == 0 {
                break;
            }
            context.push_str(separator);
            context.push_str(&truncate_utf8(&entry, remaining));
        }
        Ok((!context.is_empty()).then_some(context))
    }
}

/// Concrete routing authority backed by the existing registry snapshots.
pub(crate) struct TuiRoutingAuthority {
    pub(crate) profiles: Option<Arc<RwLock<RoutingProfileRegistry>>>,
    pub(crate) connections: Option<Arc<RoutingConnectionDirectory>>,
    pools: Option<Arc<RwLock<NamedAccountPoolRegistry>>>,
    accounts: Option<Arc<CodexAccountProfileRegistry>>,
    omni_route: Option<Arc<OmniRouteRegistry>>,
    profile_path: Option<PathBuf>,
    load_error: Option<TrustedCompositionSnapshotError>,
    session_override: Arc<RwLock<Option<RoutingProfile>>>,
}

pub(crate) struct SavedRoutingProfileState {
    registry: RoutingProfileRegistry,
    bytes: Option<Vec<u8>>,
}

impl TuiRoutingAuthority {
    pub(crate) fn unavailable() -> Self {
        Self {
            profiles: None,
            connections: None,
            pools: None,
            accounts: None,
            omni_route: None,
            profile_path: None,
            load_error: None,
            session_override: Arc::new(RwLock::new(None)),
        }
    }

    pub(crate) fn from_registry(
        profiles: RoutingProfileRegistry,
        connections: RoutingConnectionDirectory,
    ) -> Self {
        Self {
            profiles: Some(Arc::new(RwLock::new(profiles))),
            connections: Some(Arc::new(connections)),
            pools: None,
            accounts: None,
            omni_route: None,
            profile_path: None,
            load_error: None,
            session_override: Arc::new(RwLock::new(None)),
        }
    }

    fn from_loaded(
        profiles: Option<Arc<RoutingProfileRegistry>>,
        connections: Option<Arc<RoutingConnectionDirectory>>,
        pools: Option<Arc<RwLock<NamedAccountPoolRegistry>>>,
        accounts: Option<Arc<CodexAccountProfileRegistry>>,
        omni_route: Option<Arc<OmniRouteRegistry>>,
        load_error: Option<TrustedCompositionSnapshotError>,
        profile_path: Option<PathBuf>,
    ) -> Self {
        Self {
            profiles: profiles.map(|profiles| Arc::new(RwLock::new((*profiles).clone()))),
            connections,
            pools,
            accounts,
            omni_route,
            profile_path,
            load_error,
            session_override: Arc::new(RwLock::new(None)),
        }
    }

    #[allow(dead_code)]
    fn persisted_profile(&self) -> Result<RoutingProfile, TrustedCompositionSnapshotError> {
        let profiles = self
            .profiles
            .as_deref()
            .ok_or(TrustedCompositionSnapshotError::RoutingUnavailable)?;
        profiles
            .read()
            .map_err(|_| TrustedCompositionSnapshotError::RoutingUnavailable)?
            .active()
            .map(Clone::clone)
            .map_err(|_| TrustedCompositionSnapshotError::RoutingUnavailable)
    }

    pub(crate) fn current_profile(&self) -> Result<RoutingProfile, String> {
        self.session_override()
            .or_else(|| self.persisted_profile().ok())
            .ok_or_else(|| "no active routing profile is configured".to_string())
    }

    fn save_profile(&self, profile: &RoutingProfile) -> Result<SavedRoutingProfileState, String> {
        let path = self
            .profile_path
            .as_deref()
            .ok_or_else(|| "routing profile persistence is unavailable".to_string())?;
        let profiles = self
            .profiles
            .as_ref()
            .ok_or_else(|| "routing profile authority is unavailable".to_string())?;
        let previous = profiles
            .read()
            .map_err(|_| "routing profile authority is unavailable".to_string())?
            .clone();
        let previous_bytes = match std::fs::read(path) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        let mut next = previous.clone();
        if let Some(existing) = next.get_mut(&profile.id) {
            *existing = profile.clone();
        } else {
            next.insert(profile.clone())
                .map_err(|error| error.to_string())?;
        }
        next.activate(&profile.id)
            .map_err(|error| error.to_string())?;
        RoutingProfileStore::new(path.to_path_buf())
            .save(&next)
            .map_err(|error| error.to_string())?;
        *profiles
            .write()
            .map_err(|_| "routing profile authority is unavailable".to_string())? = next;
        Ok(SavedRoutingProfileState {
            registry: previous,
            bytes: previous_bytes,
        })
    }

    fn restore_profiles(&self, previous: SavedRoutingProfileState) -> Result<(), String> {
        let path = self
            .profile_path
            .as_deref()
            .ok_or_else(|| "routing profile persistence is unavailable".to_string())?;
        match previous.bytes {
            Some(bytes) => std::fs::write(path, bytes).map_err(|error| error.to_string())?,
            None => {
                if let Err(error) = std::fs::remove_file(path)
                    && error.kind() != ErrorKind::NotFound
                {
                    return Err(error.to_string());
                }
            }
        }
        let profiles = self
            .profiles
            .as_ref()
            .ok_or_else(|| "routing profile authority is unavailable".to_string())?;
        *profiles
            .write()
            .map_err(|_| "routing profile authority is unavailable".to_string())? =
            previous.registry;
        Ok(())
    }

    #[allow(dead_code)]
    fn set_session_override(
        &self,
        profile: Option<RoutingProfile>,
    ) -> Result<(), TrustedCompositionSnapshotError> {
        self.session_override
            .write()
            .map_err(|_| TrustedCompositionSnapshotError::RoutingUnavailable)
            .map(|mut current| *current = profile)
    }

    pub(crate) fn session_override(&self) -> Option<RoutingProfile> {
        self.session_override
            .read()
            .ok()
            .and_then(|profile| profile.clone())
    }

    #[allow(dead_code)]
    fn publish_session_override(
        &self,
        profile: Option<RoutingProfile>,
    ) -> Result<(), TrustedCompositionSnapshotError> {
        self.set_session_override(profile)
    }

    fn resolve_profile(
        &self,
        profile: &RoutingProfile,
    ) -> Result<RoutingProfile, TrustedCompositionSnapshotError> {
        if !profile
            .assignments
            .values()
            .any(|assignment| assignment.pool_id.is_some())
        {
            return Ok(profile.clone());
        }
        let pools = self
            .pools
            .as_deref()
            .ok_or(TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?;
        let pools = pools
            .read()
            .map_err(|_| TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?;
        let empty_accounts = CodexAccountProfileRegistry::default();
        let empty_omni_route = OmniRouteRegistry::default();
        let accounts = self.accounts.as_deref().unwrap_or(&empty_accounts);
        let omni_route = self.omni_route.as_deref().unwrap_or(&empty_omni_route);
        resolve_routing_profile(profile, &pools, accounts, omni_route)
            .map_err(|_| TrustedCompositionSnapshotError::PoolResolutionUnavailable)
    }

    fn snapshot_with_pools(
        &self,
        profile: &RoutingProfile,
        connections: &RoutingConnectionDirectory,
        pools: NamedAccountPoolRegistry,
    ) -> Result<TrustedRoutingSnapshot, TrustedCompositionSnapshotError> {
        let has_round_robin = profile.assignments.values().any(|assignment| {
            assignment.pool_id.as_ref().is_some_and(|pool_id| {
                pools.get(pool_id).is_some_and(|pool| {
                    matches!(
                        pool.selection_policy,
                        AccountPoolSelectionPolicy::RoundRobin
                    )
                })
            })
        });
        let profile = if has_round_robin {
            profile.clone()
        } else {
            self.resolve_profile(profile)?
        };
        TrustedRoutingSnapshot::from_profile_with_pools(&profile, connections, pools)
    }
}

impl fmt::Debug for TuiRoutingAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TuiRoutingAuthority")
            .field(
                "profiles",
                &self.profiles.as_ref().map(|_| "<routing-authority>"),
            )
            .field(
                "connections",
                &self.connections.as_ref().map(|_| "<connection-authority>"),
            )
            .finish()
    }
}

impl TrustedRoutingAuthority for TuiRoutingAuthority {
    fn snapshot(&self) -> Result<TrustedRoutingSnapshot, TrustedCompositionSnapshotError> {
        if let Some(error) = self.load_error {
            return Err(error);
        }
        let profiles = self
            .profiles
            .as_deref()
            .ok_or(TrustedCompositionSnapshotError::RoutingUnavailable)?;
        let profiles = profiles
            .read()
            .map_err(|_| TrustedCompositionSnapshotError::RoutingUnavailable)?;
        let connections = self
            .connections
            .as_deref()
            .ok_or(TrustedCompositionSnapshotError::RoutingUnavailable)?;
        if let Some(profile) = self.session_override() {
            if !profile
                .assignments
                .values()
                .any(|assignment| assignment.pool_id.is_some())
            {
                return TrustedRoutingSnapshot::from_profile(&profile, connections);
            }
            let Some(pools_authority) = self.pools.as_deref() else {
                return TrustedRoutingSnapshot::from_profile(&profile, connections);
            };
            let pools = pools_authority
                .read()
                .map_err(|_| TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?
                .clone();
            return self.snapshot_with_pools(&profile, connections, pools);
        }
        let profile = profiles
            .active()
            .map_err(|_| TrustedCompositionSnapshotError::RoutingUnavailable)?;
        if !profile
            .assignments
            .values()
            .any(|assignment| assignment.pool_id.is_some())
        {
            return TrustedRoutingSnapshot::from_profile(profile, connections);
        }
        let pools = self
            .pools
            .as_deref()
            .ok_or(TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?
            .read()
            .map_err(|_| TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?
            .clone();
        self.snapshot_with_pools(profile, connections, pools)
    }

    fn snapshot_for_profile(
        &self,
        profile: &RoutingProfile,
    ) -> Result<TrustedRoutingSnapshot, TrustedCompositionSnapshotError> {
        if let Some(error) = self.load_error {
            return Err(error);
        }
        let connections = self
            .connections
            .as_deref()
            .ok_or(TrustedCompositionSnapshotError::RoutingUnavailable)?;
        if !profile
            .assignments
            .values()
            .any(|assignment| assignment.pool_id.is_some())
        {
            return TrustedRoutingSnapshot::from_profile(profile, connections);
        }
        let pools = self
            .pools
            .as_deref()
            .ok_or(TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?
            .read()
            .map_err(|_| TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?
            .clone();
        self.snapshot_with_pools(profile, connections, pools)
    }
}

/// Concrete provider metadata authority. It validates exact selected
/// identities without retrieving credentials or invoking a provider.
pub(crate) struct TuiProviderAuthority {
    pub(crate) accounts: Option<Arc<CodexAccountProfileRegistry>>,
    pub(crate) omni_route: Option<Arc<OmniRouteRegistry>>,
    load_error: Option<TrustedCompositionSnapshotError>,
}

impl TuiProviderAuthority {
    pub(crate) fn unavailable() -> Self {
        Self {
            accounts: None,
            omni_route: None,
            load_error: None,
        }
    }

    pub(crate) fn from_registries(
        accounts: CodexAccountProfileRegistry,
        omni_route: OmniRouteRegistry,
    ) -> Self {
        Self {
            accounts: Some(Arc::new(accounts)),
            omni_route: Some(Arc::new(omni_route)),
            load_error: None,
        }
    }

    fn from_loaded(
        accounts: Option<Arc<CodexAccountProfileRegistry>>,
        omni_route: Option<Arc<OmniRouteRegistry>>,
        load_error: Option<TrustedCompositionSnapshotError>,
    ) -> Self {
        Self {
            accounts,
            omni_route,
            load_error,
        }
    }
}

pub(crate) struct TuiCanonicalAuthorities {
    pub(crate) routing: TuiRoutingAuthority,
    pub(crate) provider: TuiProviderAuthority,
    pub(crate) pools: TuiPoolAuthority,
}

impl TuiCanonicalAuthorities {
    pub(crate) fn load(codex_home: &Path) -> Self {
        let (profiles, profile_error) =
            match RoutingProfileRegistry::load(&codex_home.join(ROUTING_PROFILE_FILE)) {
                Ok(profiles) => (Some(Arc::new(profiles)), None),
                Err(_) => (None, Some(TrustedCompositionSnapshotError::RoutingInvalid)),
            };
        let (accounts, account_error) =
            match CodexAccountProfileRegistry::load(&codex_home.join(CODEX_ACCOUNT_FILE)) {
                Ok(accounts) => (Some(Arc::new(accounts)), None),
                Err(_) => (
                    None,
                    Some(TrustedCompositionSnapshotError::AccountAuthorityUnavailable),
                ),
            };
        let (omni_route, omni_error) =
            match OmniRouteRegistry::load(&codex_home.join(PROVIDER_CONNECTION_FILE)) {
                Ok(omni_route) => (Some(Arc::new(omni_route)), None),
                Err(_) => (
                    None,
                    Some(TrustedCompositionSnapshotError::ProviderAuthorityUnavailable),
                ),
            };
        let connections = match (omni_route.as_deref(), accounts.as_deref()) {
            (Some(omni_route), accounts) => {
                let mut directory = RoutingConnectionDirectory::from_omniroute(omni_route);
                if let Some(accounts) = accounts {
                    directory.add_codex(accounts);
                }
                Some(Arc::new(directory))
            }
            (None, Some(accounts)) => {
                let mut directory = RoutingConnectionDirectory::default();
                directory.add_codex(accounts);
                Some(Arc::new(directory))
            }
            (None, None) => None,
        };
        let pools = TuiPoolAuthority::load(codex_home, accounts.clone(), omni_route.clone());
        let routing = TuiRoutingAuthority::from_loaded(
            profiles,
            connections,
            Some(Arc::clone(&pools.registry)),
            accounts.clone(),
            omni_route.clone(),
            profile_error,
            Some(codex_home.join(ROUTING_PROFILE_FILE)),
        );
        Self {
            routing,
            provider: TuiProviderAuthority::from_loaded(
                accounts.clone(),
                omni_route.clone(),
                account_error.or(omni_error),
            ),
            pools,
        }
    }

    fn setup_snapshot(&self) -> ProviderSetupSnapshot {
        ProviderSetupSnapshot::from_authorities(self)
    }
}

impl fmt::Debug for TuiProviderAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TuiProviderAuthority")
            .field(
                "accounts",
                &self.accounts.as_ref().map(|_| "<account-authority>"),
            )
            .field(
                "omni_route",
                &self.omni_route.as_ref().map(|_| "<provider-authority>"),
            )
            .finish()
    }
}

impl TrustedProductionProviderAuthority for TuiProviderAuthority {
    fn validate_routes(
        &self,
        routing: &TrustedRoutingSnapshot,
    ) -> Result<(), TrustedCompositionSnapshotError> {
        if let Some(error) = self.load_error {
            return Err(error);
        }
        for assignment in routing.profile.assignments.values() {
            if let Some(pool_id) = &assignment.pool_id {
                let pool = routing
                    .pools
                    .as_ref()
                    .and_then(|pools| pools.get(pool_id))
                    .ok_or(TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?;
                let compatible = match assignment.provider_id.as_str() {
                    "codex" => pool.provider_family == AccountPoolProviderFamily::NativeCodex,
                    "omniroute" => pool.provider_family == AccountPoolProviderFamily::OmniRoute,
                    _ => false,
                };
                if !compatible {
                    return Err(TrustedCompositionSnapshotError::ProviderUnsupported);
                }
                pool.validate_structure()
                    .map_err(|_| TrustedCompositionSnapshotError::PoolResolutionUnavailable)?;
                if matches!(
                    pool.selection_policy,
                    codex_app_server_client::legacy_core::AccountPoolSelectionPolicy::ExplicitMember(_)
                ) {
                    let accounts = self.accounts.as_deref().cloned().unwrap_or_default();
                    let omni_route = self.omni_route.as_deref().cloned().unwrap_or_default();
                    routing
                        .pools
                        .as_ref()
                        .and_then(|pools| pools.resolve_pool(pool_id, &accounts, &omni_route).ok())
                        .ok_or(TrustedCompositionSnapshotError::PoolResolutionUnavailable)?;
                }
                continue;
            }
            routing
                .connections
                .validate_assignment(assignment)
                .map_err(|_| TrustedCompositionSnapshotError::ConnectionAuthorityUnavailable)?;
            match assignment.provider_id.as_str() {
                "codex" => {
                    let accounts = self
                        .accounts
                        .as_deref()
                        .ok_or(TrustedCompositionSnapshotError::ProviderAuthorityUnavailable)?;
                    let account = accounts
                        .get_connection(&assignment.connection_id)
                        .ok_or(TrustedCompositionSnapshotError::AccountAuthorityUnavailable)?;
                    if account.state != CodexAccountProfileState::Connected
                        || !account.enabled
                        || account.validation != ConnectionValidationStatus::Valid
                    {
                        return Err(
                            TrustedCompositionSnapshotError::ConnectionAuthorityUnavailable,
                        );
                    }
                }
                "omniroute" => {
                    let registry = self
                        .omni_route
                        .as_deref()
                        .ok_or(TrustedCompositionSnapshotError::ProviderAuthorityUnavailable)?;
                    let connection = registry
                        .get(&assignment.connection_id)
                        .ok_or(TrustedCompositionSnapshotError::ConnectionAuthorityUnavailable)?;
                    if !connection.enabled {
                        return Err(
                            TrustedCompositionSnapshotError::ConnectionAuthorityUnavailable,
                        );
                    }
                }
                "openrouter" => {
                    return Err(TrustedCompositionSnapshotError::ProviderAuthorityUnavailable);
                }
                _ => return Err(TrustedCompositionSnapshotError::ProviderAuthorityUnavailable),
            }
        }
        Ok(())
    }

    fn construction_snapshot(
        &self,
        routing: &TrustedRoutingSnapshot,
        policy: &ResolvedExecutionPolicy,
    ) -> Result<ProductionProviderConstructionSnapshot, TrustedCompositionSnapshotError> {
        self.validate_routes(routing)?;
        let accounts = self.accounts.as_deref();
        let omni_route = self.omni_route.as_deref();
        let mut bindings = BTreeMap::new();
        let mut round_robin_bindings = BTreeMap::new();
        for (role, assignment) in &routing.profile.assignments {
            if let Some(pool_id) = &assignment.pool_id {
                let pool = routing
                    .pools
                    .as_ref()
                    .and_then(|pools| pools.get(pool_id))
                    .ok_or(TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?;
                let accounts_snapshot = accounts.cloned().unwrap_or_default();
                let omni_route_snapshot = omni_route.cloned().unwrap_or_default();
                if matches!(
                    pool.selection_policy,
                    codex_app_server_client::legacy_core::AccountPoolSelectionPolicy::ExplicitMember(_)
                ) {
                    let member = routing
                        .pools
                        .as_ref()
                        .ok_or(TrustedCompositionSnapshotError::PoolAuthorityUnavailable)?
                        .resolve_pool(pool_id, &accounts_snapshot, &omni_route_snapshot)
                        .map_err(|_| TrustedCompositionSnapshotError::PoolResolutionUnavailable)?;
                    let connection_id = match member.target {
                        codex_app_server_client::legacy_core::AccountPoolTarget::NativeCodexAccount(
                            profile_id,
                        ) => accounts_snapshot
                            .get(&profile_id)
                            .ok_or(TrustedCompositionSnapshotError::AccountAuthorityUnavailable)?
                            .connection_id
                            .clone(),
                        codex_app_server_client::legacy_core::AccountPoolTarget::OmniRouteConnection(
                            connection_id,
                        ) => connection_id,
                    };
                    let selection = ProviderSelection::new(
                        connection_id,
                        assignment.provider_id.clone(),
                        assignment.model_id.clone(),
                    )
                    .map_err(|_| TrustedCompositionSnapshotError::ProviderConstructionUnavailable)?;
                    let route = ProductionProviderRoute::new(selection, policy.role(*role).effort.clone());
                    let binding = match assignment.provider_id.as_str() {
                        "codex" => native_codex_binding(route, accounts_snapshot.clone()),
                        "omniroute" => {
                            let connection = omni_route_snapshot
                                .get(&route.selection().connection_id)
                                .ok_or(TrustedCompositionSnapshotError::ConnectionAuthorityUnavailable)?;
                            omniroute_binding(route, connection.clone())
                        }
                        _ => Err(ProviderConstructionError::UnsupportedProvider),
                    }
                    .map_err(|_| TrustedCompositionSnapshotError::ProviderConstructionUnavailable)?;
                    bindings.insert(*role, binding);
                    continue;
                }
                let selection = ProviderSelection::new(
                    format!("pool-{pool_id}"),
                    assignment.provider_id.clone(),
                    assignment.model_id.clone(),
                )
                .map_err(|_| TrustedCompositionSnapshotError::ProviderConstructionUnavailable)?;
                let route =
                    ProductionProviderRoute::new(selection, policy.role(*role).effort.clone());
                let binding = ProductionRoundRobinProviderBinding::new(
                    route,
                    pool.clone(),
                    accounts_snapshot,
                    omni_route_snapshot,
                )
                .map_err(|_| TrustedCompositionSnapshotError::ProviderConstructionUnavailable)?;
                round_robin_bindings.insert(*role, binding);
                continue;
            }
            let selection = ProviderSelection::new(
                assignment.connection_id.clone(),
                assignment.provider_id.clone(),
                assignment.model_id.clone(),
            )
            .map_err(|_| TrustedCompositionSnapshotError::ProviderConstructionUnavailable)?;
            let route = ProductionProviderRoute::new(selection, policy.role(*role).effort.clone());
            let binding = match assignment.provider_id.as_str() {
                "codex" => {
                    let accounts = accounts
                        .ok_or(TrustedCompositionSnapshotError::ProviderAuthorityUnavailable)?;
                    if !codex_app_server_client::legacy_core::codex_auth_exists(
                        &assignment.connection_id,
                    )
                    .map_err(|_| TrustedCompositionSnapshotError::ProviderConstructionUnavailable)?
                    {
                        return Err(TrustedCompositionSnapshotError::AccountUnauthenticated);
                    }
                    native_codex_binding(route, accounts.clone())
                }
                "omniroute" => {
                    let registry = omni_route
                        .ok_or(TrustedCompositionSnapshotError::ProviderAuthorityUnavailable)?;
                    let connection = registry
                        .get(&assignment.connection_id)
                        .ok_or(TrustedCompositionSnapshotError::ConnectionAuthorityUnavailable)?;
                    if !codex_app_server_client::legacy_core::omniroute_credential_exists(
                        connection,
                    )
                    .map_err(|_| TrustedCompositionSnapshotError::ProviderConstructionUnavailable)?
                    {
                        return Err(TrustedCompositionSnapshotError::AccountUnauthenticated);
                    }
                    omniroute_binding(route, connection.clone())
                }
                "openrouter" => openrouter_binding(route),
                _ => Err(ProviderConstructionError::UnsupportedProvider),
            }
            .map_err(|error| match error {
                ProviderConstructionError::UnsupportedProvider
                | ProviderConstructionError::OpenRouterUnsupported => {
                    TrustedCompositionSnapshotError::ProviderUnsupported
                }
                ProviderConstructionError::AccountUnauthenticated => {
                    TrustedCompositionSnapshotError::AccountUnauthenticated
                }
                ProviderConstructionError::AccountMissing => {
                    TrustedCompositionSnapshotError::AccountAuthorityUnavailable
                }
                ProviderConstructionError::ConnectionMissing => {
                    TrustedCompositionSnapshotError::ConnectionAuthorityUnavailable
                }
                ProviderConstructionError::NativeCodexConstructionUnavailable
                | ProviderConstructionError::OmniRouteConstructionUnavailable
                | ProviderConstructionError::AuthenticationAuthorityUnavailable
                | ProviderConstructionError::ModelUnavailable
                | ProviderConstructionError::UnsupportedEffort
                | ProviderConstructionError::ProviderAuthorityMismatch
                | ProviderConstructionError::RoundRobinMemberUnavailable => {
                    TrustedCompositionSnapshotError::ProviderConstructionUnavailable
                }
            })?;
            bindings.insert(*role, binding);
        }
        Ok(ProductionProviderConstructionSnapshot::new(bindings)
            .with_round_robin(round_robin_bindings))
    }
}

/// Concrete approved-tool authority. The absence of a role-capability
/// snapshot remains unavailable rather than becoming a permissive default.
pub(crate) struct TuiApprovedToolAuthority {
    capabilities: Option<Arc<ValidatedRoleCapabilitySet>>,
    workspace_root: Option<PathBuf>,
    load_error: Option<TrustedCompositionSnapshotError>,
}

impl TuiApprovedToolAuthority {
    pub(crate) fn unavailable() -> Self {
        Self {
            capabilities: None,
            workspace_root: None,
            load_error: None,
        }
    }

    pub(crate) fn from_validated(
        capabilities: ValidatedRoleCapabilitySet,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            capabilities: Some(Arc::new(capabilities)),
            workspace_root: Some(workspace_root),
            load_error: None,
        }
    }

    pub(crate) fn from_persisted(
        codex_home: &Path,
        policy: &codex_app_server_client::legacy_core::ResolvedExecutionPolicy,
        context: &RoleCapabilityValidationContext,
    ) -> Self {
        match load_role_capabilities(codex_home, policy, context) {
            Ok(capabilities) => {
                Self::from_validated(capabilities, context.workspace_root().to_path_buf())
            }
            Err(RoleCapabilityConfigError::Unavailable) => Self {
                capabilities: None,
                workspace_root: None,
                load_error: Some(
                    TrustedCompositionSnapshotError::RoleCapabilityConfigurationUnavailable,
                ),
            },
            Err(_) => Self {
                capabilities: None,
                workspace_root: None,
                load_error: Some(
                    TrustedCompositionSnapshotError::RoleCapabilityConfigurationInvalid,
                ),
            },
        }
    }
}

impl fmt::Debug for TuiApprovedToolAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TuiApprovedToolAuthority")
            .field(
                "capabilities",
                &self.capabilities.as_ref().map(|_| "<tool-authority>"),
            )
            .finish()
    }
}

impl TrustedApprovedToolAuthority for TuiApprovedToolAuthority {
    fn snapshot(
        &self,
        workspace_root: &Path,
    ) -> Result<TrustedApprovedToolSnapshot, TrustedCompositionSnapshotError> {
        let capabilities = self.capabilities.as_ref().ok_or_else(|| {
            self.load_error
                .unwrap_or(TrustedCompositionSnapshotError::RoleCapabilityAuthorityUnavailable)
        })?;
        if self.workspace_root.as_deref() != Some(workspace_root) {
            return Err(TrustedCompositionSnapshotError::WorkspaceUnavailable);
        }
        Ok(TrustedApprovedToolSnapshot::from_validated(
            capabilities.as_ref().clone(),
        ))
    }
}

/// TUI-owned composition state that holds one trusted source for one session.
pub(crate) struct TuiSyndridSessionComposition {
    source: Arc<TrustedSyndridCompositionSource>,
    routing_authority: Option<Arc<TuiRoutingAuthority>>,
    context_provider: Arc<TuiProductionContextProvider>,
    policy_state: Arc<SessionExecutionPolicyState>,
    workspace_root: PathBuf,
    runtime: Mutex<Option<Arc<ProductionSessionRuntime>>>,
    setup_snapshot: ProviderSetupSnapshot,
    pool_authority: Option<Arc<TuiPoolAuthority>>,
}

/// A fully prepared session routing update. It is not authoritative until published after the
/// trusted runtime installation boundary succeeds.
#[allow(dead_code)]
pub(crate) struct PreparedSessionRoutingUpdate {
    override_profile: Option<RoutingProfile>,
    runtime: Option<Arc<ProductionSessionRuntime>>,
}

pub(crate) enum RoutingApplyMode {
    SessionOnly,
    SaveActiveProfile,
}

impl PreparedSessionRoutingUpdate {
    #[allow(dead_code)]
    pub(crate) fn runtime(&self) -> Option<Arc<ProductionSessionRuntime>> {
        self.runtime.clone()
    }
}

impl fmt::Debug for TuiSyndridSessionComposition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TuiSyndridSessionComposition")
            .field("source", &"<trusted-composition-source>")
            .field("context_provider", &"<context-provider>")
            .field(
                "runtime",
                &self
                    .runtime
                    .lock()
                    .ok()
                    .and_then(|runtime| runtime.as_ref().map(|_| "<session-runtime>")),
            )
            .finish()
    }
}

impl TuiSyndridSessionComposition {
    pub(crate) fn new(
        session_id: String,
        workspace_root: PathBuf,
        policy_state: Arc<SessionExecutionPolicyState>,
        event_sender: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<Self, TrustedCompositionSnapshotError> {
        let context_provider = Arc::new(TuiProductionContextProvider::new());
        Self::new_with_authorities(
            session_id,
            workspace_root,
            policy_state,
            Arc::new(TuiRoutingAuthority::unavailable()),
            Arc::new(TuiProviderAuthority::unavailable()),
            Arc::new(TuiApprovedToolAuthority::unavailable()),
            context_provider,
            event_sender,
        )
    }

    pub(crate) fn new_from_canonical_home(
        session_id: String,
        workspace_root: PathBuf,
        codex_home: &Path,
        policy_state: Arc<SessionExecutionPolicyState>,
        event_sender: mpsc::Sender<InProcessServerEvent>,
        capability_context: RoleCapabilityValidationContext,
    ) -> Result<Self, TrustedCompositionSnapshotError> {
        let authorities = TuiCanonicalAuthorities::load(codex_home);
        let setup_snapshot = authorities.setup_snapshot();
        let context_provider = Arc::new(TuiProductionContextProvider::new());
        let policy = policy_state
            .resolved_policy()
            .map_err(|_| TrustedCompositionSnapshotError::PolicyInvalid)?;
        let tool_authority =
            TuiApprovedToolAuthority::from_persisted(codex_home, &policy, &capability_context);
        let routing_authority = Arc::new(authorities.routing);
        let mut composition = Self::new_with_authorities(
            session_id,
            workspace_root,
            policy_state,
            routing_authority.clone(),
            Arc::new(authorities.provider),
            Arc::new(tool_authority),
            context_provider,
            event_sender,
        )?;
        composition.routing_authority = Some(routing_authority);
        composition.setup_snapshot = setup_snapshot;
        composition.pool_authority = Some(Arc::new(authorities.pools));
        Ok(composition)
    }

    pub(crate) fn new_with_authorities(
        session_id: String,
        workspace_root: PathBuf,
        policy_state: Arc<SessionExecutionPolicyState>,
        routing_authority: Arc<dyn TrustedRoutingAuthority>,
        provider_authority: Arc<dyn TrustedProductionProviderAuthority>,
        tool_authority: Arc<dyn TrustedApprovedToolAuthority>,
        context_provider: Arc<TuiProductionContextProvider>,
        event_sender: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<Self, TrustedCompositionSnapshotError> {
        let runtime_policy_state = policy_state.clone();
        let dependencies = TrustedSyndridCompositionDependencies {
            session_id,
            workspace_root: workspace_root.clone(),
            policy_state: Some(policy_state),
            routing_authority: Some(routing_authority),
            provider_authority: Some(provider_authority),
            tool_authority: Some(tool_authority),
            context_provider: Some(
                context_provider.clone() as Arc<dyn ProductionTurnContextProvider>
            ),
            event_sender,
        };
        let source = Arc::new(TrustedSyndridCompositionSource::new(dependencies)?);
        let runtime = runtime_policy_state
            .resolved_orchestration_policy()
            .ok()
            .filter(|policy| policy.requires_syndrid_runtime())
            .and_then(|_| {
                source
                    .snapshot(TrustedCompositionSnapshotRequest {
                        session_id: source.session_id().to_owned(),
                        workspace_root: workspace_root.clone(),
                    })
                    .ok()
                    .and_then(|snapshot| {
                        assemble_trusted_production_runtime(
                            &snapshot,
                            (*runtime_policy_state).clone(),
                        )
                        .ok()
                        .map(Arc::new)
                    })
            });
        Ok(Self {
            source,
            routing_authority: None,
            context_provider,
            policy_state: runtime_policy_state,
            workspace_root,
            runtime: Mutex::new(runtime),
            setup_snapshot: ProviderSetupSnapshot::unavailable(),
            pool_authority: None,
        })
    }

    pub(crate) fn source(&self) -> Arc<TrustedSyndridCompositionSource> {
        Arc::clone(&self.source)
    }

    pub(crate) fn context_provider(&self) -> Arc<TuiProductionContextProvider> {
        Arc::clone(&self.context_provider)
    }

    pub(crate) fn runtime(&self) -> Option<Arc<ProductionSessionRuntime>> {
        self.runtime.lock().ok().and_then(|runtime| runtime.clone())
    }

    #[allow(dead_code)]
    pub(crate) fn session_routing_override(&self) -> Option<RoutingProfile> {
        self.routing_authority
            .as_ref()
            .and_then(|authority| authority.session_override())
    }

    pub(crate) fn current_routing_profile(&self) -> Result<RoutingProfile, String> {
        self.routing_authority
            .as_ref()
            .ok_or_else(|| "session routing authority is unavailable".to_string())?
            .current_profile()
    }

    /// Prepares an exact session routing override without publishing it or installing a runtime.
    #[allow(dead_code)]
    pub(crate) fn prepare_session_routing_override(
        &self,
        profile: RoutingProfile,
    ) -> Result<PreparedSessionRoutingUpdate, String> {
        self.routing_authority
            .as_ref()
            .ok_or_else(|| "session routing authority is unavailable".to_string())?;
        let policy = self
            .policy_state
            .resolved_orchestration_policy()
            .map_err(|error| error.to_string())?;
        if !policy.requires_syndrid_runtime() {
            return Ok(PreparedSessionRoutingUpdate {
                override_profile: Some(profile),
                runtime: None,
            });
        }
        let snapshot = self
            .source
            .snapshot_with_policy_and_routing(
                TrustedCompositionSnapshotRequest {
                    session_id: self.source.session_id().to_owned(),
                    workspace_root: self.workspace_root.clone(),
                },
                &self.policy_state,
                &profile,
            )
            .map_err(|error| error.to_string())?;
        let runtime = Some(Arc::new(
            assemble_trusted_production_runtime(&snapshot, (*self.policy_state).clone())
                .map_err(|error| error.to_string())?,
        ));
        Ok(PreparedSessionRoutingUpdate {
            override_profile: Some(profile),
            runtime,
        })
    }

    /// Prepares restoration of the active persisted routing profile without publishing it.
    #[allow(dead_code)]
    pub(crate) fn prepare_clear_session_routing_override(
        &self,
    ) -> Result<PreparedSessionRoutingUpdate, String> {
        let policy = self
            .policy_state
            .resolved_orchestration_policy()
            .map_err(|error| error.to_string())?;
        let routing_authority = self
            .routing_authority
            .as_ref()
            .ok_or_else(|| "session routing authority is unavailable".to_string())?;
        let profile = routing_authority
            .persisted_profile()
            .map_err(|error| error.to_string())?;
        if !policy.requires_syndrid_runtime() {
            return Ok(PreparedSessionRoutingUpdate {
                override_profile: None,
                runtime: None,
            });
        }
        let snapshot = self
            .source
            .snapshot_with_policy_and_routing(
                TrustedCompositionSnapshotRequest {
                    session_id: self.source.session_id().to_owned(),
                    workspace_root: self.workspace_root.clone(),
                },
                &self.policy_state,
                &profile,
            )
            .map_err(|error| error.to_string())?;
        let runtime = Some(Arc::new(
            assemble_trusted_production_runtime(&snapshot, (*self.policy_state).clone())
                .map_err(|error| error.to_string())?,
        ));
        Ok(PreparedSessionRoutingUpdate {
            override_profile: None,
            runtime,
        })
    }

    /// Publishes a prepared routing update after its runtime has been installed successfully.
    #[allow(dead_code)]
    fn publish_session_routing_update(
        &self,
        prepared: PreparedSessionRoutingUpdate,
    ) -> Result<(), String> {
        let routing_authority = self
            .routing_authority
            .as_ref()
            .ok_or_else(|| "session routing authority is unavailable".to_string())?;
        let mut runtime = self
            .runtime
            .lock()
            .map_err(|_| "session runtime is unavailable".to_string())?;
        routing_authority
            .publish_session_override(prepared.override_profile)
            .map_err(|error| error.to_string())?;
        *runtime = prepared.runtime;
        Ok(())
    }

    /// Installs a prepared runtime and publishes its routing only after installation succeeds.
    /// The installer is injected so the rollback contract can be tested without starting a
    /// provider or an app-server worker.
    #[allow(dead_code)]
    pub(crate) async fn install_prepared_session_routing_update<F, Fut>(
        &self,
        capability: ProductionExecutionCapability,
        prepared: PreparedSessionRoutingUpdate,
        install: F,
    ) -> Result<(), String>
    where
        F: Fn(ProductionExecutionCapability, Option<Arc<ProductionSessionRuntime>>) -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        let _routing_guard = self
            .policy_state
            .begin_routing_update()
            .map_err(|error| error.to_string())?;
        let previous_runtime = self.runtime();
        install(capability, prepared.runtime()).await?;
        if let Err(error) = self.publish_session_routing_update(prepared) {
            install(capability, previous_runtime)
                .await
                .map_err(|restore_error| {
                    format!("{error}; previous runtime restoration failed: {restore_error}")
                })?;
            return Err(error);
        }
        Ok(())
    }

    /// Installs a prepared routing update while holding the idle reservation across the
    /// persisted-profile write, runtime installation, and session publication.
    pub(crate) async fn install_prepared_session_routing_update_and_save<F, Fut>(
        &self,
        capability: ProductionExecutionCapability,
        prepared: PreparedSessionRoutingUpdate,
        profile: &RoutingProfile,
        install: F,
    ) -> Result<(), String>
    where
        F: Fn(ProductionExecutionCapability, Option<Arc<ProductionSessionRuntime>>) -> Fut,
        Fut: Future<Output = Result<(), String>>,
    {
        let _routing_guard = self
            .policy_state
            .begin_routing_update()
            .map_err(|error| error.to_string())?;
        let authority = self
            .routing_authority
            .as_ref()
            .ok_or_else(|| "session routing authority is unavailable".to_string())?;
        let previous_profiles = authority.save_profile(profile)?;
        let previous_runtime = self.runtime();
        if let Err(error) = install(capability, prepared.runtime()).await {
            let runtime_error = install(capability, previous_runtime).await.err();
            let profile_error = authority.restore_profiles(previous_profiles).err();
            return Err(match (runtime_error, profile_error) {
                (None, None) => error,
                (Some(runtime_error), None) => {
                    format!("{error}; previous runtime restoration failed: {runtime_error}")
                }
                (None, Some(profile_error)) => {
                    format!("{error}; previous routing profile restoration failed: {profile_error}")
                }
                (Some(runtime_error), Some(profile_error)) => format!(
                    "{error}; previous runtime restoration failed: {runtime_error}; previous routing profile restoration failed: {profile_error}"
                ),
            });
        }
        if let Err(error) = self.publish_session_routing_update(prepared) {
            let runtime_error = install(capability, previous_runtime).await.err();
            let profile_error = authority.restore_profiles(previous_profiles).err();
            return Err(match (runtime_error, profile_error) {
                (None, None) => error,
                (Some(runtime_error), None) => {
                    format!("{error}; previous runtime restoration failed: {runtime_error}")
                }
                (None, Some(profile_error)) => {
                    format!("{error}; previous routing profile restoration failed: {profile_error}")
                }
                (Some(runtime_error), Some(profile_error)) => format!(
                    "{error}; previous runtime restoration failed: {runtime_error}; previous routing profile restoration failed: {profile_error}"
                ),
            });
        }
        Ok(())
    }

    pub(crate) fn provider_setup_snapshot(&self) -> &ProviderSetupSnapshot {
        &self.setup_snapshot
    }

    pub(crate) fn pool_authority(&self) -> Option<Arc<TuiPoolAuthority>> {
        self.pool_authority.clone()
    }

    pub(crate) fn execution_capability(&self) -> ProductionExecutionCapability {
        match self
            .policy_state
            .strategy()
            .unwrap_or(OrchestrationMode::Single)
        {
            OrchestrationMode::Single => ProductionExecutionCapability::CodexCompatibility,
            OrchestrationMode::Manual
            | OrchestrationMode::Recommended
            | OrchestrationMode::Automatic
            | OrchestrationMode::Adaptive => ProductionExecutionCapability::SyndridOrchestration,
        }
    }

    /// Validates a candidate orchestration runtime without publishing it or invoking providers.
    pub(crate) fn validate_runtime_for_selection(
        &self,
        strategy: OrchestrationMode,
        preset: ExecutionModeSelection,
    ) -> Result<(), String> {
        let candidate_state = (*self.policy_state).clone();
        candidate_state
            .select_strategy(strategy)
            .map_err(|error| error.to_string())?;
        candidate_state
            .select_mode(
                preset,
                codex_app_server_client::legacy_core::SessionPolicySource::SessionOverride,
            )
            .map_err(|error| error.to_string())?;
        let policy = candidate_state
            .resolved_orchestration_policy()
            .map_err(|error| error.to_string())?;
        if !policy.requires_syndrid_runtime() {
            return Ok(());
        }
        let snapshot = self
            .source
            .snapshot_with_policy_state(
                TrustedCompositionSnapshotRequest {
                    session_id: self.source.session_id().to_owned(),
                    workspace_root: self.workspace_root.clone(),
                },
                &candidate_state,
            )
            .map_err(|error| error.to_string())?;
        assemble_trusted_production_runtime(&snapshot, candidate_state)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn validate_runtime_for_routing_selection(
        &self,
        strategy: OrchestrationMode,
        preset: ExecutionModeSelection,
        profile: &RoutingProfile,
    ) -> Result<(), String> {
        let candidate_state = (*self.policy_state).clone();
        candidate_state
            .select_strategy(strategy)
            .map_err(|error| error.to_string())?;
        candidate_state
            .select_mode(
                preset,
                codex_app_server_client::legacy_core::SessionPolicySource::SessionOverride,
            )
            .map_err(|error| error.to_string())?;
        let policy = candidate_state
            .resolved_orchestration_policy()
            .map_err(|error| error.to_string())?;
        if !policy.requires_syndrid_runtime() {
            return Ok(());
        }
        let snapshot = self
            .source
            .snapshot_with_policy_and_routing(
                TrustedCompositionSnapshotRequest {
                    session_id: self.source.session_id().to_owned(),
                    workspace_root: self.workspace_root.clone(),
                },
                &candidate_state,
                profile,
            )
            .map_err(|error| error.to_string())?;
        assemble_trusted_production_runtime(&snapshot, candidate_state)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn refresh_runtime(&self) -> Result<(), String> {
        let policy = self
            .policy_state
            .resolved_orchestration_policy()
            .map_err(|error| error.to_string())?;
        let runtime = if policy.requires_syndrid_runtime() {
            let snapshot = self
                .source
                .snapshot(TrustedCompositionSnapshotRequest {
                    session_id: self.source.session_id().to_owned(),
                    workspace_root: self.workspace_root.clone(),
                })
                .map_err(|error| error.to_string())?;
            Some(Arc::new(
                assemble_trusted_production_runtime(&snapshot, (*self.policy_state).clone())
                    .map_err(|error| error.to_string())?,
            ))
        } else {
            None
        };
        let mut installed = self
            .runtime
            .lock()
            .map_err(|_| "session runtime is unavailable".to_string())?;
        *installed = runtime;
        Ok(())
    }
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[cfg(test)]
#[path = "syndrid_composition_tests.rs"]
mod tests;
