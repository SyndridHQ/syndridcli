//! Trusted, non-serialized Syndrid composition snapshots.
//!
//! This module carries references to authorities owned by an embedded product
//! composition root. It does not discover credentials, invoke providers or
//! tools, or select an execution path. The outer composition root remains the
//! authority that decides which policy, routing, provider, and capability
//! state is valid for a session.

use std::fmt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use codex_app_server::ProductionTurnContextProvider;
use codex_app_server::in_process::InProcessServerEvent;
use codex_core::OrchestrationMode;
use codex_core::OrchestrationStrategyAvailability;
use codex_core::ProductionProviderConstructionSnapshot;
use codex_core::ResolvedExecutionPolicy;
use codex_core::RoutingConnectionDirectory;
use codex_core::RoutingProfile;
use codex_core::RoutingProfileId;
use codex_core::RoutingProfileRegistry;
use codex_core::SessionExecutionPolicyState;
use codex_core::SessionPolicyValidation;
use codex_core::ValidatedRoleCapabilitySet;
use tokio::sync::mpsc;

/// A validated, immutable view of the active routing authority for one snapshot.
///
/// The directory contains connection metadata only. Provider credentials and
/// invocation clients remain owned by the trusted provider authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedRoutingSnapshot {
    /// The profile selected by the trusted product authority.
    pub profile_id: RoutingProfileId,
    /// The validated role assignments captured for this snapshot.
    pub profile: RoutingProfile,
    /// The connection metadata used to validate those assignments.
    pub connections: RoutingConnectionDirectory,
}

impl TrustedRoutingSnapshot {
    /// Captures the active profile and connection metadata without performing I/O.
    pub fn from_registry(
        profiles: &RoutingProfileRegistry,
        connections: &RoutingConnectionDirectory,
    ) -> Result<Self, TrustedCompositionSnapshotError> {
        let profile = profiles
            .active()
            .map_err(|_| TrustedCompositionSnapshotError::RoutingUnavailable)?;
        Self::from_profile(profile, connections)
    }

    /// Captures one already-selected immutable profile and validates it against the connection
    /// directory without mutating the persisted registry.
    pub fn from_profile(
        profile: &RoutingProfile,
        connections: &RoutingConnectionDirectory,
    ) -> Result<Self, TrustedCompositionSnapshotError> {
        profile
            .validate_required_roles()
            .map_err(|_| TrustedCompositionSnapshotError::RoutingInvalid)?;
        for assignment in profile.assignments.values() {
            connections
                .validate_assignment(assignment)
                .map_err(|_| TrustedCompositionSnapshotError::RoutingInvalid)?;
        }
        Ok(Self {
            profile_id: profile.id.clone(),
            profile: profile.clone(),
            connections: connections.clone(),
        })
    }
}

/// Provides a validated routing snapshot from the product's existing routing authority.
pub trait TrustedRoutingAuthority: Send + Sync {
    /// Captures current routing exactly once for a future turn.
    fn snapshot(&self) -> Result<TrustedRoutingSnapshot, TrustedCompositionSnapshotError>;

    /// Validates and captures a caller-owned candidate profile without publishing it.
    ///
    /// Authorities that support session-owned routing overrides should implement this method.
    /// The default keeps candidate routing unavailable for authorities that only expose their
    /// active persisted profile.
    fn snapshot_for_profile(
        &self,
        _profile: &RoutingProfile,
    ) -> Result<TrustedRoutingSnapshot, TrustedCompositionSnapshotError> {
        Err(TrustedCompositionSnapshotError::RoutingUnavailable)
    }
}

/// Provides exact provider availability for an already-selected routing snapshot.
///
/// Implementations may inspect existing registries and client ownership, but
/// must not invoke a provider, select a fallback, rotate accounts, or expose
/// credentials through this contract.
pub trait TrustedProductionProviderAuthority: Send + Sync {
    /// Validates that the selected routes can be resolved by existing clients.
    fn validate_routes(
        &self,
        routing: &TrustedRoutingSnapshot,
    ) -> Result<(), TrustedCompositionSnapshotError>;

    /// Captures deferred, exact construction authorities without invoking a provider.
    fn construction_snapshot(
        &self,
        routing: &TrustedRoutingSnapshot,
        policy: &ResolvedExecutionPolicy,
    ) -> Result<ProductionProviderConstructionSnapshot, TrustedCompositionSnapshotError> {
        self.validate_routes(routing)?;
        let _ = policy;
        Err(TrustedCompositionSnapshotError::ProviderConstructionUnavailable)
    }
}

/// Provides the immutable approved-tool capability envelope for a session.
///
/// This is a capability snapshot operation, not tool execution. Implementations
/// must preserve existing approval, workspace, role, output, and budget rules.
pub trait TrustedApprovedToolAuthority: Send + Sync {
    /// Captures approved capabilities for the supplied workspace boundary.
    fn snapshot(
        &self,
        workspace_root: &Path,
    ) -> Result<TrustedApprovedToolSnapshot, TrustedCompositionSnapshotError>;
}

/// Immutable, non-serialized approved role capabilities for one trusted session.
#[derive(Clone)]
pub struct TrustedApprovedToolSnapshot {
    pub role_capabilities: ValidatedRoleCapabilitySet,
}

impl fmt::Debug for TrustedApprovedToolSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrustedApprovedToolSnapshot")
            .field("role_capabilities", &"<redacted>")
            .finish()
    }
}

impl TrustedApprovedToolSnapshot {
    pub fn from_validated(role_capabilities: ValidatedRoleCapabilitySet) -> Self {
        Self { role_capabilities }
    }
}

/// Inputs supplied by one trusted embedded product session.
///
/// Optional fields make unavailable authority explicit at the snapshot
/// boundary. They are never replaced with defaults or fabricated state.
pub struct TrustedSyndridCompositionDependencies {
    /// Stable identity of the trusted embedded session.
    pub session_id: String,
    /// Session-owned workspace boundary.
    pub workspace_root: PathBuf,
    /// The shared policy state also used by the product UI.
    pub policy_state: Option<Arc<SessionExecutionPolicyState>>,
    /// Existing validated routing authority.
    pub routing_authority: Option<Arc<dyn TrustedRoutingAuthority>>,
    /// Existing provider/account/connection authority.
    pub provider_authority: Option<Arc<dyn TrustedProductionProviderAuthority>>,
    /// Existing approved-tool authority.
    pub tool_authority: Option<Arc<dyn TrustedApprovedToolAuthority>>,
    /// Product-owned bounded conversation context source.
    pub context_provider: Option<Arc<dyn ProductionTurnContextProvider>>,
    /// Existing in-process event destination for this session.
    pub event_sender: mpsc::Sender<InProcessServerEvent>,
}

impl fmt::Debug for TrustedSyndridCompositionDependencies {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrustedSyndridCompositionDependencies")
            .field("session_id", &"<redacted>")
            .field("workspace_root", &"<redacted>")
            .field(
                "policy_state",
                &self.policy_state.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "routing_authority",
                &self.routing_authority.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "provider_authority",
                &self.provider_authority.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "tool_authority",
                &self.tool_authority.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "context_provider",
                &self.context_provider.as_ref().map(|_| "<redacted>"),
            )
            .field("event_sender", &"<redacted>")
            .finish()
    }
}

/// The immutable authoritative inputs captured for one future production turn.
///
/// Context remains a provider reference and is captured later at turn
/// preparation. No transcript, observation, tool output, or credential is
/// copied into this snapshot.
pub struct AuthoritativeSyndridCompositionSnapshot {
    /// Session identity that all later runtime objects must preserve.
    pub session_id: String,
    /// Validated policy cloned from the shared session authority.
    pub policy: ResolvedExecutionPolicy,
    /// Strategy selected by the trusted session authority.
    pub strategy: OrchestrationMode,
    /// Typed availability of the selected strategy.
    pub strategy_availability: OrchestrationStrategyAvailability,
    /// Validation result captured with the selected execution preset.
    pub preset_validation: SessionPolicyValidation,
    /// Immutable routing and connection metadata.
    pub routing: TrustedRoutingSnapshot,
    /// Existing provider authority, without credential material.
    pub provider_authority: Arc<dyn TrustedProductionProviderAuthority>,
    /// Deferred provider construction authorities for the captured routes.
    pub provider_construction: ProductionProviderConstructionSnapshot,
    /// Approved tool capability envelope.
    pub approved_tools: TrustedApprovedToolSnapshot,
    /// Bounded context source captured at the next turn boundary.
    pub context_provider: Arc<dyn ProductionTurnContextProvider>,
    /// Session-owned workspace boundary.
    pub workspace_root: PathBuf,
    /// Existing neutral event destination.
    pub event_sender: mpsc::Sender<InProcessServerEvent>,
}

impl fmt::Debug for AuthoritativeSyndridCompositionSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoritativeSyndridCompositionSnapshot")
            .field("session_id", &"<redacted>")
            .field("policy", &"<redacted>")
            .field("routing", &"<redacted>")
            .field("provider_authority", &"<redacted>")
            .field("approved_tools", &"<redacted>")
            .field("context_provider", &"<redacted>")
            .field("workspace_root", &"<redacted>")
            .field("event_sender", &"<redacted>")
            .finish()
    }
}

impl Clone for AuthoritativeSyndridCompositionSnapshot {
    fn clone(&self) -> Self {
        Self {
            session_id: self.session_id.clone(),
            policy: self.policy.clone(),
            strategy: self.strategy,
            strategy_availability: self.strategy_availability,
            preset_validation: self.preset_validation.clone(),
            routing: self.routing.clone(),
            provider_authority: Arc::clone(&self.provider_authority),
            provider_construction: self.provider_construction.clone(),
            approved_tools: self.approved_tools.clone(),
            context_provider: Arc::clone(&self.context_provider),
            workspace_root: self.workspace_root.clone(),
            event_sender: self.event_sender.clone(),
        }
    }
}

/// Typed failures raised while capturing trusted product state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustedCompositionSnapshotError {
    /// The session identity is empty or exceeds the admission bound.
    InvalidSessionIdentity,
    /// The session request does not belong to this source.
    SessionMismatch,
    /// The workspace boundary is empty or not absolute.
    WorkspaceUnavailable,
    /// Shared policy state was not installed.
    PolicyUnavailable,
    /// Shared policy state could not provide a valid snapshot.
    PolicyInvalid,
    /// Routing state was not installed.
    RoutingUnavailable,
    /// Routing state failed validation.
    RoutingInvalid,
    /// Provider/account/connection authority was not installed.
    ProviderAuthorityUnavailable,
    /// Provider connection authority could not resolve a selected route.
    ConnectionAuthorityUnavailable,
    /// Provider account authority could not resolve a selected account.
    AccountAuthorityUnavailable,
    /// A selected provider has no safe construction authority.
    ProviderConstructionUnavailable,
    /// A selected provider is unsupported by the existing production adapters.
    ProviderUnsupported,
    /// A selected account is not authenticated in the existing authority.
    AccountUnauthenticated,
    /// Role-capability authority was not installed.
    RoleCapabilityAuthorityUnavailable,
    /// Persisted role-capability configuration was unavailable.
    RoleCapabilityConfigurationUnavailable,
    /// Persisted role-capability configuration was invalid.
    RoleCapabilityConfigurationInvalid,
    /// Context authority was not installed.
    ContextAuthorityUnavailable,
}

impl fmt::Display for TrustedCompositionSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidSessionIdentity => "trusted session identity is invalid",
            Self::SessionMismatch => "trusted composition session does not match the request",
            Self::WorkspaceUnavailable => "trusted workspace boundary is unavailable",
            Self::PolicyUnavailable => "trusted execution policy is unavailable",
            Self::PolicyInvalid => "trusted execution policy is invalid",
            Self::RoutingUnavailable => "trusted routing authority is unavailable",
            Self::RoutingInvalid => "trusted routing snapshot is invalid",
            Self::ProviderAuthorityUnavailable => "trusted provider authority is unavailable",
            Self::ConnectionAuthorityUnavailable => {
                "trusted provider connection authority is unavailable"
            }
            Self::AccountAuthorityUnavailable => {
                "trusted provider account authority is unavailable"
            }
            Self::ProviderConstructionUnavailable => {
                "trusted provider construction authority is unavailable"
            }
            Self::ProviderUnsupported => "trusted provider is unsupported",
            Self::AccountUnauthenticated => "trusted provider account is unauthenticated",
            Self::RoleCapabilityAuthorityUnavailable => {
                "trusted role-capability authority is unavailable"
            }
            Self::RoleCapabilityConfigurationUnavailable => {
                "trusted role-capability configuration is unavailable"
            }
            Self::RoleCapabilityConfigurationInvalid => {
                "trusted role-capability configuration is invalid"
            }
            Self::ContextAuthorityUnavailable => "trusted context authority is unavailable",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for TrustedCompositionSnapshotError {}

/// Trusted session-scoped source that snapshots existing product authorities.
pub struct TrustedSyndridCompositionSource {
    dependencies: TrustedSyndridCompositionDependencies,
}

impl fmt::Debug for TrustedSyndridCompositionSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrustedSyndridCompositionSource")
            .field("dependencies", &"<redacted>")
            .finish()
    }
}

impl TrustedSyndridCompositionSource {
    /// Creates a source without resolving or invoking any authority.
    pub fn new(
        dependencies: TrustedSyndridCompositionDependencies,
    ) -> Result<Self, TrustedCompositionSnapshotError> {
        validate_session_id(&dependencies.session_id)?;
        validate_workspace(&dependencies.workspace_root)?;
        Ok(Self { dependencies })
    }

    /// Captures one immutable authority snapshot for the requested session.
    pub fn snapshot(
        &self,
        request: TrustedCompositionSnapshotRequest,
    ) -> Result<AuthoritativeSyndridCompositionSnapshot, TrustedCompositionSnapshotError> {
        let policy_state = self
            .dependencies
            .policy_state
            .as_ref()
            .ok_or(TrustedCompositionSnapshotError::PolicyUnavailable)?;
        self.snapshot_with_policy_state(request, policy_state)
    }

    /// Captures trusted authorities against a caller-owned candidate policy without publishing it.
    ///
    /// The candidate must be derived from the same session policy authority. This seam is used by
    /// local setup readiness to validate a future runtime without mutating the installed policy.
    pub fn snapshot_with_policy_state(
        &self,
        request: TrustedCompositionSnapshotRequest,
        policy_state: &SessionExecutionPolicyState,
    ) -> Result<AuthoritativeSyndridCompositionSnapshot, TrustedCompositionSnapshotError> {
        self.snapshot_with_policy_state_and_routing(request, policy_state, None)
    }

    /// Captures trusted authorities against a caller-owned policy and routing profile without
    /// publishing either candidate to the session.
    pub fn snapshot_with_policy_and_routing(
        &self,
        request: TrustedCompositionSnapshotRequest,
        policy_state: &SessionExecutionPolicyState,
        routing_profile: &RoutingProfile,
    ) -> Result<AuthoritativeSyndridCompositionSnapshot, TrustedCompositionSnapshotError> {
        self.snapshot_with_policy_state_and_routing(request, policy_state, Some(routing_profile))
    }

    fn snapshot_with_policy_state_and_routing(
        &self,
        request: TrustedCompositionSnapshotRequest,
        policy_state: &SessionExecutionPolicyState,
        routing_profile: Option<&RoutingProfile>,
    ) -> Result<AuthoritativeSyndridCompositionSnapshot, TrustedCompositionSnapshotError> {
        if request.session_id != self.dependencies.session_id {
            return Err(TrustedCompositionSnapshotError::SessionMismatch);
        }
        if request.workspace_root != self.dependencies.workspace_root {
            return Err(TrustedCompositionSnapshotError::WorkspaceUnavailable);
        }

        let orchestration_policy = policy_state
            .resolved_orchestration_policy()
            .map_err(|_| TrustedCompositionSnapshotError::PolicyInvalid)?;
        let preset_validation = policy_state
            .validation()
            .map_err(|_| TrustedCompositionSnapshotError::PolicyInvalid)?;
        let policy = orchestration_policy.execution().clone();
        let routing_authority = self
            .dependencies
            .routing_authority
            .as_ref()
            .ok_or(TrustedCompositionSnapshotError::RoutingUnavailable)?;
        let routing = routing_profile.map_or_else(
            || routing_authority.snapshot(),
            |profile| routing_authority.snapshot_for_profile(profile),
        )?;
        policy
            .validate_routing_profile(&routing.profile, &routing.connections)
            .map_err(|_| TrustedCompositionSnapshotError::RoutingInvalid)?;

        let provider_authority = self
            .dependencies
            .provider_authority
            .as_ref()
            .ok_or(TrustedCompositionSnapshotError::ProviderAuthorityUnavailable)?;
        let provider_construction = provider_authority.construction_snapshot(&routing, &policy)?;
        let tool_authority = self
            .dependencies
            .tool_authority
            .as_ref()
            .ok_or(TrustedCompositionSnapshotError::RoleCapabilityAuthorityUnavailable)?;
        let approved_tools = tool_authority.snapshot(&self.dependencies.workspace_root)?;
        let context_provider = self
            .dependencies
            .context_provider
            .as_ref()
            .ok_or(TrustedCompositionSnapshotError::ContextAuthorityUnavailable)?;

        Ok(AuthoritativeSyndridCompositionSnapshot {
            session_id: self.dependencies.session_id.clone(),
            policy,
            strategy: orchestration_policy.strategy(),
            strategy_availability: orchestration_policy.availability(),
            preset_validation,
            routing,
            provider_authority: Arc::clone(provider_authority),
            provider_construction,
            approved_tools,
            context_provider: Arc::clone(context_provider),
            workspace_root: self.dependencies.workspace_root.clone(),
            event_sender: self.dependencies.event_sender.clone(),
        })
    }

    /// Returns the owning session identity without exposing other state.
    pub fn session_id(&self) -> &str {
        &self.dependencies.session_id
    }
}

/// Admission metadata used to request one composition snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrustedCompositionSnapshotRequest {
    /// Session identity supplied by the trusted in-process caller.
    pub session_id: String,
    /// Workspace identity supplied by the same trusted session.
    pub workspace_root: PathBuf,
}

fn validate_session_id(session_id: &str) -> Result<(), TrustedCompositionSnapshotError> {
    if session_id.trim().is_empty() || session_id.len() > 256 {
        return Err(TrustedCompositionSnapshotError::InvalidSessionIdentity);
    }
    Ok(())
}

fn validate_workspace(path: &Path) -> Result<(), TrustedCompositionSnapshotError> {
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(TrustedCompositionSnapshotError::WorkspaceUnavailable);
    }
    Ok(())
}

#[cfg(test)]
#[path = "trusted_composition_tests.rs"]
mod tests;
