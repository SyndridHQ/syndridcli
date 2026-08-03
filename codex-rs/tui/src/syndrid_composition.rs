//! Trusted, session-scoped Syndrid composition owned by the TUI.
//!
//! This module supplies product-owned authority implementations to the neutral
//! app-server-client seam. It assembles inert runtime state for a future turn;
//! it does not select the production turn path or invoke a runner.

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
use codex_app_server_client::legacy_core::CodexAccountProfileRegistry;
use codex_app_server_client::legacy_core::CodexAccountProfileState;
use codex_app_server_client::legacy_core::ConnectionValidationStatus;
use codex_app_server_client::legacy_core::ExecutionModeSelection;
use codex_app_server_client::legacy_core::OmniRouteRegistry;
use codex_app_server_client::legacy_core::OrchestrationMode;
use codex_app_server_client::legacy_core::ProductionProviderConstructionSnapshot;
use codex_app_server_client::legacy_core::ProductionProviderRoute;
use codex_app_server_client::legacy_core::ProviderConstructionError;
use codex_app_server_client::legacy_core::ProviderSelection;
use codex_app_server_client::legacy_core::ResolvedExecutionPolicy;
use codex_app_server_client::legacy_core::RoleCapabilityConfigError;
use codex_app_server_client::legacy_core::RoleCapabilityValidationContext;
use codex_app_server_client::legacy_core::RoutingConnectionDirectory;
use codex_app_server_client::legacy_core::RoutingProfileRegistry;
use codex_app_server_client::legacy_core::SessionExecutionPolicyState;
use codex_app_server_client::legacy_core::ValidatedRoleCapabilitySet;
use codex_app_server_client::legacy_core::load_role_capabilities;
use codex_app_server_client::legacy_core::native_codex_binding;
use codex_app_server_client::legacy_core::omniroute_binding;
use codex_app_server_client::legacy_core::openrouter_binding;
use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::fmt;
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
    profiles: Option<Arc<RoutingProfileRegistry>>,
    connections: Option<Arc<RoutingConnectionDirectory>>,
    load_error: Option<TrustedCompositionSnapshotError>,
}

impl TuiRoutingAuthority {
    pub(crate) fn unavailable() -> Self {
        Self {
            profiles: None,
            connections: None,
            load_error: None,
        }
    }

    pub(crate) fn from_registry(
        profiles: RoutingProfileRegistry,
        connections: RoutingConnectionDirectory,
    ) -> Self {
        Self {
            profiles: Some(Arc::new(profiles)),
            connections: Some(Arc::new(connections)),
            load_error: None,
        }
    }

    fn from_loaded(
        profiles: Option<Arc<RoutingProfileRegistry>>,
        connections: Option<Arc<RoutingConnectionDirectory>>,
        load_error: Option<TrustedCompositionSnapshotError>,
    ) -> Self {
        Self {
            profiles,
            connections,
            load_error,
        }
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
        let connections = self
            .connections
            .as_deref()
            .ok_or(TrustedCompositionSnapshotError::RoutingUnavailable)?;
        TrustedRoutingSnapshot::from_registry(profiles, connections)
    }
}

/// Concrete provider metadata authority. It validates exact selected
/// identities without retrieving credentials or invoking a provider.
pub(crate) struct TuiProviderAuthority {
    accounts: Option<Arc<CodexAccountProfileRegistry>>,
    omni_route: Option<Arc<OmniRouteRegistry>>,
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

struct TuiCanonicalAuthorities {
    routing: TuiRoutingAuthority,
    provider: TuiProviderAuthority,
}

impl TuiCanonicalAuthorities {
    fn load(codex_home: &Path) -> Self {
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
        Self {
            routing: TuiRoutingAuthority::from_loaded(profiles, connections, profile_error),
            provider: TuiProviderAuthority::from_loaded(
                accounts,
                omni_route,
                account_error.or(omni_error),
            ),
        }
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
        for (role, assignment) in &routing.profile.assignments {
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
                | ProviderConstructionError::ProviderAuthorityMismatch => {
                    TrustedCompositionSnapshotError::ProviderConstructionUnavailable
                }
            })?;
            bindings.insert(*role, binding);
        }
        Ok(ProductionProviderConstructionSnapshot::new(bindings))
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
    context_provider: Arc<TuiProductionContextProvider>,
    policy_state: Arc<SessionExecutionPolicyState>,
    workspace_root: PathBuf,
    runtime: Mutex<Option<Arc<ProductionSessionRuntime>>>,
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
        let context_provider = Arc::new(TuiProductionContextProvider::new());
        let policy = policy_state
            .resolved_policy()
            .map_err(|_| TrustedCompositionSnapshotError::PolicyInvalid)?;
        let tool_authority =
            TuiApprovedToolAuthority::from_persisted(codex_home, &policy, &capability_context);
        Self::new_with_authorities(
            session_id,
            workspace_root,
            policy_state,
            Arc::new(authorities.routing),
            Arc::new(authorities.provider),
            Arc::new(tool_authority),
            context_provider,
            event_sender,
        )
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
            context_provider,
            policy_state: runtime_policy_state,
            workspace_root,
            runtime: Mutex::new(runtime),
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
