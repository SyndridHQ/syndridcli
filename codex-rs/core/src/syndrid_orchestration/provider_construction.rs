use super::account_pools::AccountPoolSelectionPolicy;
use super::account_pools::AccountPoolTarget;
use super::account_pools::NamedAccountPool;
use super::codex_accounts::CodexAccountProfileRegistry;
use super::codex_invocation::CODEX_PROVIDER_ID;
use super::codex_invocation::CodexInvocationAdapter;
use super::omniroute::OmniRouteConnectionMetadata;
use super::omniroute::OmniRouteRegistry;
use super::omniroute::native_omniroute_adapter;
use super::production_dispatch::ProductionRoleBinding;
use super::production_request::ProductionProviderAdapter;
use super::production_request::ProductionProviderRoute;
use super::routing_profiles::RoutingRole;
use super::scoped_codex_session::ScopedCodexInvocationClient;
use super::strategy_candidates::RoutingStrategyCandidate;
use std::collections::BTreeMap;
use std::fmt;

/// Provider construction failures that can be established without invoking a provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProviderConstructionError {
    #[error("provider is unsupported")]
    UnsupportedProvider,
    #[error("provider connection is missing")]
    ConnectionMissing,
    #[error("provider account is missing")]
    AccountMissing,
    #[error("provider account is unauthenticated")]
    AccountUnauthenticated,
    #[error("provider authentication authority is unavailable")]
    AuthenticationAuthorityUnavailable,
    #[error("provider model is unavailable")]
    ModelUnavailable,
    #[error("provider effort is unsupported")]
    UnsupportedEffort,
    #[error("native Codex construction authority is unavailable")]
    NativeCodexConstructionUnavailable,
    #[error("OmniRoute construction authority is unavailable")]
    OmniRouteConstructionUnavailable,
    #[error("OpenRouter is unsupported")]
    OpenRouterUnsupported,
    #[error("provider construction route does not match the captured route")]
    ProviderAuthorityMismatch,
    #[error("round-robin pool member is unavailable")]
    RoundRobinMemberUnavailable,
}

/// A deferred, exact provider authority for one captured role route.
///
/// The authority owns only non-secret metadata and existing registry handles. Its binding
/// method creates the existing provider adapter without retrieving credentials or making a
/// provider request. Invocation remains the responsibility of a later runtime milestone.
#[derive(Clone)]
pub struct ProductionProviderConstructionBinding {
    route: ProductionProviderRoute,
    authority: ProductionProviderConstructionAuthority,
}

impl fmt::Debug for ProductionProviderConstructionBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductionProviderConstructionBinding")
            .field("route", &"<redacted>")
            .field("authority", &"<redacted>")
            .finish()
    }
}

#[derive(Clone)]
enum ProductionProviderConstructionAuthority {
    NativeCodex {
        accounts: CodexAccountProfileRegistry,
    },
    OmniRoute {
        connection: OmniRouteConnectionMetadata,
    },
    OpenRouterUnavailable,
}

impl fmt::Debug for ProductionProviderConstructionAuthority {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::NativeCodex { .. } => "NativeCodex",
            Self::OmniRoute { .. } => "OmniRoute",
            Self::OpenRouterUnavailable => "OpenRouterUnavailable",
        };
        formatter.write_str(name)
    }
}

impl ProductionProviderConstructionBinding {
    pub fn route(&self) -> &ProductionProviderRoute {
        &self.route
    }

    pub fn build(&self) -> Result<ProductionRoleBinding, ProviderConstructionError> {
        match &self.authority {
            ProductionProviderConstructionAuthority::NativeCodex { accounts } => {
                let provider = CodexInvocationAdapter::new(
                    self.route.selection().clone(),
                    accounts.clone(),
                    ScopedCodexInvocationClient,
                )
                .map_err(|_| ProviderConstructionError::NativeCodexConstructionUnavailable)?;
                Ok(ProductionRoleBinding::new(self.route.clone(), provider))
            }
            ProductionProviderConstructionAuthority::OmniRoute { connection } => {
                let provider = native_omniroute_adapter(connection.clone())
                    .map_err(|_| ProviderConstructionError::OmniRouteConstructionUnavailable)?;
                let provider = ProductionProviderAdapter::new(self.route.clone(), provider)
                    .map_err(|_| ProviderConstructionError::OmniRouteConstructionUnavailable)?;
                Ok(ProductionRoleBinding::new(self.route.clone(), provider))
            }
            ProductionProviderConstructionAuthority::OpenRouterUnavailable => {
                Err(ProviderConstructionError::OpenRouterUnsupported)
            }
        }
    }
}

/// Deferred construction authority for one structurally valid RoundRobin role route.
///
/// The pool and provider registries are immutable snapshots. No member is selected and no
/// provider is constructed until the production turn boundary supplies an exact member ID.
#[derive(Clone)]
pub struct ProductionRoundRobinProviderBinding {
    route: ProductionProviderRoute,
    pool: NamedAccountPool,
    accounts: CodexAccountProfileRegistry,
    connections: OmniRouteRegistry,
}

impl fmt::Debug for ProductionRoundRobinProviderBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductionRoundRobinProviderBinding")
            .field("route", &"<redacted>")
            .field("pool_id", &self.pool.id)
            .finish_non_exhaustive()
    }
}

impl ProductionRoundRobinProviderBinding {
    pub fn new(
        route: ProductionProviderRoute,
        pool: NamedAccountPool,
        accounts: CodexAccountProfileRegistry,
        connections: OmniRouteRegistry,
    ) -> Result<Self, ProviderConstructionError> {
        pool.validate_structure()
            .map_err(|_| ProviderConstructionError::RoundRobinMemberUnavailable)?;
        if !matches!(
            pool.selection_policy,
            AccountPoolSelectionPolicy::RoundRobin
        ) {
            return Err(ProviderConstructionError::ProviderAuthorityMismatch);
        }
        Ok(Self {
            route,
            pool,
            accounts,
            connections,
        })
    }

    pub fn route(&self) -> &ProductionProviderRoute {
        &self.route
    }

    pub fn pool_id(&self) -> &super::account_pools::PoolId {
        &self.pool.id
    }

    pub(crate) fn pool(&self) -> &NamedAccountPool {
        &self.pool
    }

    pub(crate) fn target_for_member(
        &self,
        member_id: &super::account_pools::PoolMemberId,
    ) -> Result<AccountPoolTarget, ProviderConstructionError> {
        self.pool
            .members
            .iter()
            .find(|member| &member.id == member_id)
            .map(|member| member.target.clone())
            .ok_or(ProviderConstructionError::RoundRobinMemberUnavailable)
    }

    /// Resolves and constructs exactly the requested configured member.
    pub fn build_for_member(
        &self,
        member_id: &super::account_pools::PoolMemberId,
    ) -> Result<ProductionRoleBinding, ProviderConstructionError> {
        let member = self
            .pool
            .members
            .iter()
            .find(|member| &member.id == member_id)
            .ok_or(ProviderConstructionError::RoundRobinMemberUnavailable)?;
        let expected_provider = match member.target.provider_family() {
            super::account_pools::AccountPoolProviderFamily::NativeCodex => CODEX_PROVIDER_ID,
            super::account_pools::AccountPoolProviderFamily::OmniRoute => "omniroute",
        };
        if self.route.selection().provider_id != expected_provider {
            return Err(ProviderConstructionError::ProviderAuthorityMismatch);
        }
        let connection_id = match &member.target {
            AccountPoolTarget::NativeCodexAccount(profile_id) => self
                .accounts
                .get(profile_id)
                .filter(|profile| profile.enabled)
                .map(|profile| profile.connection_id.clone())
                .ok_or(ProviderConstructionError::AccountMissing)?,
            AccountPoolTarget::OmniRouteConnection(connection_id) => self
                .connections
                .get(connection_id)
                .filter(|connection| connection.enabled)
                .map(|_| connection_id.clone())
                .ok_or(ProviderConstructionError::ConnectionMissing)?,
        };
        let selection = super::omniroute::ProviderSelection::new(
            &connection_id,
            self.route.selection().provider_id.clone(),
            self.route.selection().model_id.clone(),
        )
        .map_err(|_| ProviderConstructionError::ProviderAuthorityMismatch)?;
        let route = ProductionProviderRoute::new(selection, self.route.effort());
        match &member.target {
            AccountPoolTarget::NativeCodexAccount(profile_id) => {
                let _ = profile_id;
                native_codex_binding(route, self.accounts.clone())?
                    .build()
                    .map(|binding| binding.with_cooldown_target(member.target.clone()))
            }
            AccountPoolTarget::OmniRouteConnection(connection_id) => {
                let connection = self
                    .connections
                    .get(connection_id)
                    .ok_or(ProviderConstructionError::ConnectionMissing)?;
                omniroute_binding(route, connection.clone())?
                    .build()
                    .map(|binding| binding.with_cooldown_target(member.target.clone()))
            }
        }
    }
}

/// Immutable provider construction authorities captured for one routing snapshot.
#[derive(Clone)]
pub struct ProductionProviderConstructionSnapshot {
    bindings: BTreeMap<RoutingRole, ProductionProviderConstructionBinding>,
    round_robin_bindings: BTreeMap<RoutingRole, ProductionRoundRobinProviderBinding>,
    automatic_candidates:
        BTreeMap<RoutingRole, Vec<ProductionAutomaticProviderConstructionCandidate>>,
}

/// A deferred construction authority for one explicitly ordered Automatic candidate.
#[derive(Clone)]
pub enum ProductionAutomaticProviderConstructionBinding {
    Direct(ProductionProviderConstructionBinding),
    RoundRobin(ProductionRoundRobinProviderBinding),
}

impl fmt::Debug for ProductionAutomaticProviderConstructionBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Direct(_) => formatter.write_str("Direct"),
            Self::RoundRobin(binding) => binding.fmt(formatter),
        }
    }
}

/// One safe Automatic candidate identity paired with deferred provider construction.
#[derive(Clone, Debug)]
pub struct ProductionAutomaticProviderConstructionCandidate {
    candidate: RoutingStrategyCandidate,
    binding: ProductionAutomaticProviderConstructionBinding,
}

impl ProductionAutomaticProviderConstructionCandidate {
    pub fn new(
        candidate: RoutingStrategyCandidate,
        binding: ProductionAutomaticProviderConstructionBinding,
    ) -> Self {
        Self { candidate, binding }
    }

    pub fn candidate(&self) -> &RoutingStrategyCandidate {
        &self.candidate
    }

    pub fn binding(&self) -> &ProductionAutomaticProviderConstructionBinding {
        &self.binding
    }
}

impl fmt::Debug for ProductionProviderConstructionSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductionProviderConstructionSnapshot")
            .field("role_count", &self.bindings.len())
            .field("round_robin_role_count", &self.round_robin_bindings.len())
            .field("automatic_role_count", &self.automatic_candidates.len())
            .field("bindings", &"<redacted>")
            .finish()
    }
}

impl ProductionProviderConstructionSnapshot {
    /// Creates an immutable snapshot from exact, already validated role bindings.
    pub fn new(bindings: BTreeMap<RoutingRole, ProductionProviderConstructionBinding>) -> Self {
        Self {
            bindings,
            round_robin_bindings: BTreeMap::new(),
            automatic_candidates: BTreeMap::new(),
        }
    }

    pub fn with_round_robin(
        mut self,
        bindings: BTreeMap<RoutingRole, ProductionRoundRobinProviderBinding>,
    ) -> Self {
        self.round_robin_bindings = bindings;
        self
    }

    pub fn with_automatic_candidates(
        mut self,
        candidates: BTreeMap<RoutingRole, Vec<ProductionAutomaticProviderConstructionCandidate>>,
    ) -> Self {
        self.automatic_candidates = candidates;
        self
    }

    /// Returns the deferred binding for one exact role.
    pub fn binding(
        &self,
        role: RoutingRole,
    ) -> Result<&ProductionProviderConstructionBinding, ProviderConstructionError> {
        self.bindings
            .get(&role)
            .ok_or(ProviderConstructionError::ProviderAuthorityMismatch)
    }

    /// Builds one existing provider binding without invoking a provider.
    pub fn build_role_binding(
        &self,
        role: RoutingRole,
    ) -> Result<ProductionRoleBinding, ProviderConstructionError> {
        self.binding(role)?.build()
    }

    pub fn round_robin_binding(
        &self,
        role: RoutingRole,
    ) -> Result<&ProductionRoundRobinProviderBinding, ProviderConstructionError> {
        self.round_robin_bindings
            .get(&role)
            .ok_or(ProviderConstructionError::ProviderAuthorityMismatch)
    }

    pub fn is_round_robin(&self, role: RoutingRole) -> bool {
        self.round_robin_bindings.contains_key(&role)
    }

    pub fn automatic_candidates(
        &self,
        role: RoutingRole,
    ) -> &[ProductionAutomaticProviderConstructionCandidate] {
        self.automatic_candidates
            .get(&role)
            .map_or(&[], Vec::as_slice)
    }

    /// Returns the captured roles in deterministic order.
    pub fn roles(&self) -> impl Iterator<Item = RoutingRole> + '_ {
        self.bindings
            .keys()
            .chain(self.round_robin_bindings.keys())
            .copied()
    }
}

/// Builds a native Codex construction authority after exact account validation.
pub fn native_codex_binding(
    route: ProductionProviderRoute,
    accounts: CodexAccountProfileRegistry,
) -> Result<ProductionProviderConstructionBinding, ProviderConstructionError> {
    if route.selection().provider_id != CODEX_PROVIDER_ID {
        return Err(ProviderConstructionError::UnsupportedProvider);
    }
    let account = accounts
        .get_connection(&route.selection().connection_id)
        .ok_or(ProviderConstructionError::AccountMissing)?;
    if account.provider_id != CODEX_PROVIDER_ID {
        return Err(ProviderConstructionError::UnsupportedProvider);
    }
    if !account.enabled {
        return Err(ProviderConstructionError::AccountUnauthenticated);
    }
    if account.account_id.is_none() {
        return Err(ProviderConstructionError::AccountMissing);
    }
    Ok(ProductionProviderConstructionBinding {
        route,
        authority: ProductionProviderConstructionAuthority::NativeCodex { accounts },
    })
}

/// Builds an OmniRoute construction authority after exact connection validation.
pub fn omniroute_binding(
    route: ProductionProviderRoute,
    connection: OmniRouteConnectionMetadata,
) -> Result<ProductionProviderConstructionBinding, ProviderConstructionError> {
    if route.selection().provider_id != connection.provider_id
        || route.selection().connection_id != connection.connection_id
        || !connection.enabled
        || !connection
            .models
            .iter()
            .any(|model| model == &route.selection().model_id)
    {
        return Err(ProviderConstructionError::ConnectionMissing);
    }
    Ok(ProductionProviderConstructionBinding {
        route,
        authority: ProductionProviderConstructionAuthority::OmniRoute { connection },
    })
}

/// Represents an explicitly unsupported OpenRouter route.
pub fn openrouter_binding(
    route: ProductionProviderRoute,
) -> Result<ProductionProviderConstructionBinding, ProviderConstructionError> {
    if route.selection().provider_id != "openrouter" {
        return Err(ProviderConstructionError::UnsupportedProvider);
    }
    Ok(ProductionProviderConstructionBinding {
        route,
        authority: ProductionProviderConstructionAuthority::OpenRouterUnavailable,
    })
}

#[cfg(test)]
#[path = "provider_construction_tests.rs"]
mod tests;
