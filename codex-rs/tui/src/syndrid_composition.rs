//! Trusted, session-scoped Syndrid composition owned by the TUI.
//!
//! This module supplies product-owned authority implementations to the neutral
//! app-server-client seam. It only captures state for a future milestone; it
//! does not select the production turn path or construct a runner.

use codex_app_server_client::InProcessServerEvent;
use codex_app_server_client::ProductionTurnAdmissionInput;
use codex_app_server_client::ProductionTurnContextProvider;
use codex_app_server_client::ProductionTurnPreparationError;
use codex_app_server_client::TrustedApprovedToolAuthority;
use codex_app_server_client::TrustedApprovedToolSnapshot;
use codex_app_server_client::TrustedCompositionSnapshotError;
use codex_app_server_client::TrustedProductionProviderAuthority;
use codex_app_server_client::TrustedRoutingAuthority;
use codex_app_server_client::TrustedRoutingSnapshot;
use codex_app_server_client::TrustedSyndridCompositionDependencies;
use codex_app_server_client::TrustedSyndridCompositionSource;
use codex_app_server_client::legacy_core::CodexAccountProfileRegistry;
use codex_app_server_client::legacy_core::CodexAccountProfileState;
use codex_app_server_client::legacy_core::ConnectionValidationStatus;
use codex_app_server_client::legacy_core::OmniRouteRegistry;
use codex_app_server_client::legacy_core::RoutingConnectionDirectory;
use codex_app_server_client::legacy_core::RoutingProfileRegistry;
use codex_app_server_client::legacy_core::SessionExecutionPolicyState;
use codex_app_server_client::legacy_core::ValidatedRoleCapabilitySet;
use std::collections::VecDeque;
use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
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
}

/// Concrete approved-tool authority. The absence of a role-capability
/// snapshot remains unavailable rather than becoming a permissive default.
pub(crate) struct TuiApprovedToolAuthority {
    capabilities: Option<Arc<ValidatedRoleCapabilitySet>>,
    workspace_root: Option<PathBuf>,
}

impl TuiApprovedToolAuthority {
    pub(crate) fn unavailable() -> Self {
        Self {
            capabilities: None,
            workspace_root: None,
        }
    }

    pub(crate) fn from_validated(
        capabilities: ValidatedRoleCapabilitySet,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            capabilities: Some(Arc::new(capabilities)),
            workspace_root: Some(workspace_root),
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
        let capabilities = self
            .capabilities
            .as_ref()
            .ok_or(TrustedCompositionSnapshotError::RoleCapabilityAuthorityUnavailable)?;
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
}

impl fmt::Debug for TuiSyndridSessionComposition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TuiSyndridSessionComposition")
            .field("source", &"<trusted-composition-source>")
            .field("context_provider", &"<context-provider>")
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
    ) -> Result<Self, TrustedCompositionSnapshotError> {
        let authorities = TuiCanonicalAuthorities::load(codex_home);
        let context_provider = Arc::new(TuiProductionContextProvider::new());
        Self::new_with_authorities(
            session_id,
            workspace_root,
            policy_state,
            Arc::new(authorities.routing),
            Arc::new(authorities.provider),
            Arc::new(TuiApprovedToolAuthority::unavailable()),
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
        let dependencies = TrustedSyndridCompositionDependencies {
            session_id,
            workspace_root,
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
        Ok(Self {
            source,
            context_provider,
        })
    }

    pub(crate) fn source(&self) -> Arc<TrustedSyndridCompositionSource> {
        Arc::clone(&self.source)
    }

    pub(crate) fn context_provider(&self) -> Arc<TuiProductionContextProvider> {
        Arc::clone(&self.context_provider)
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
