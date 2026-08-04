use super::NamedAccountPoolRegistry;
use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use super::production_request::ProductionProviderRoute;
use super::provider_construction::ProductionRoundRobinProviderBinding;
use super::rotation_state::AccountPoolRotationState;
use super::routing_profiles::RoutingRole;
use super::session_execution::SessionExecutionPolicyState;
use super::subagent::SubagentProvider;
use codex_protocol::openai_models::ReasoningEffort;
use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

type InvocationFuture =
    Pin<Box<dyn Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send>>;
type InvocationFn =
    dyn Fn(ProviderInvocationRequest, CancellationToken) -> InvocationFuture + Send + Sync;

/// A bounded invocation envelope carrying the immutable route identity for one orchestration role.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionRoleInvocationRequest {
    pub role: RoutingRole,
    pub connection_id: String,
    pub account_id: Option<String>,
    pub effort: ReasoningEffort,
    pub request: ProviderInvocationRequest,
}

impl ProductionRoleInvocationRequest {
    pub fn new(
        role: RoutingRole,
        route: &ProductionProviderRoute,
        request: ProviderInvocationRequest,
        account_id: Option<String>,
    ) -> Self {
        Self {
            role,
            connection_id: route.selection().connection_id.clone(),
            account_id,
            effort: route.effort(),
            request,
        }
    }
}

#[derive(Clone)]
pub struct ProductionRoleBinding {
    route: ProductionProviderRoute,
    invoke: Arc<InvocationFn>,
}

impl fmt::Debug for ProductionRoleBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductionRoleBinding")
            .field("route", &self.route)
            .finish_non_exhaustive()
    }
}

impl ProductionRoleBinding {
    /// Binds one exact route to an existing provider-neutral implementation.
    pub fn new<P>(route: ProductionProviderRoute, provider: P) -> Self
    where
        P: SubagentProvider + 'static,
    {
        let provider = Arc::new(provider);
        let invoke = Arc::new(move |request, cancellation| {
            let provider = Arc::clone(&provider);
            Box::pin(async move { provider.invoke(request, cancellation).await })
                as InvocationFuture
        });
        Self { route, invoke }
    }

    pub fn route(&self) -> &ProductionProviderRoute {
        &self.route
    }
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProductionRoleDispatchError {
    #[error("role route is duplicated")]
    DuplicateRole,
    #[error("role route is missing for {0}")]
    MissingRole(RoutingRole),
    #[error("provider route mismatch for {role}")]
    ProviderMismatch { role: RoutingRole },
    #[error("provider connection mismatch for {role}")]
    ConnectionMismatch { role: RoutingRole },
    #[error("provider account mismatch for {role}")]
    AccountMismatch { role: RoutingRole },
    #[error("provider model mismatch for {role}")]
    ModelMismatch { role: RoutingRole },
    #[error("provider effort mismatch for {role}")]
    EffortMismatch { role: RoutingRole },
    #[error("provider invocation failed for {role}: {source}")]
    ProviderFailure {
        role: RoutingRole,
        source: ProviderInvocationError,
    },
    #[error("provider returned a result for the wrong route for {role}")]
    ResultRouteMismatch { role: RoutingRole },
    #[error("round-robin selection failed for {role}")]
    RoundRobinSelection { role: RoutingRole },
}

/// Dispatches each orchestration role through its immutable, exact provider route.
///
/// This type performs route validation and delegation only. It does not resolve profiles or
/// policy, create tools, own cancellation, publish observations, or translate final results.
#[derive(Clone, Debug, Default)]
pub struct ProductionRoleDispatcher {
    bindings: BTreeMap<RoutingRole, ProductionRoleBinding>,
    round_robin_bindings: BTreeMap<RoutingRole, ProductionRoundRobinProviderBinding>,
    rotation_state: Arc<Mutex<AccountPoolRotationState>>,
    turn_cache: Arc<tokio::sync::Mutex<BTreeMap<RoutingRole, ProductionRoleBinding>>>,
    selection_gate: Arc<tokio::sync::Mutex<()>>,
    session_state: Option<SessionExecutionPolicyState>,
}

impl ProductionRoleDispatcher {
    pub fn new<I>(bindings: I) -> Result<Self, ProductionRoleDispatchError>
    where
        I: IntoIterator<Item = (RoutingRole, ProductionRoleBinding)>,
    {
        let mut resolved = BTreeMap::new();
        for (role, binding) in bindings {
            if resolved.insert(role, binding).is_some() {
                return Err(ProductionRoleDispatchError::DuplicateRole);
            }
        }
        Ok(Self {
            bindings: resolved,
            round_robin_bindings: BTreeMap::new(),
            rotation_state: Arc::new(Mutex::new(AccountPoolRotationState::new())),
            turn_cache: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            selection_gate: Arc::new(tokio::sync::Mutex::new(())),
            session_state: None,
        })
    }

    pub fn with_round_robin<I, J>(
        bindings: I,
        round_robin_bindings: J,
        rotation_state: Arc<Mutex<AccountPoolRotationState>>,
    ) -> Result<Self, ProductionRoleDispatchError>
    where
        I: IntoIterator<Item = (RoutingRole, ProductionRoleBinding)>,
        J: IntoIterator<Item = (RoutingRole, ProductionRoundRobinProviderBinding)>,
    {
        let mut dispatcher = Self::new(bindings)?;
        for (role, binding) in round_robin_bindings {
            if dispatcher
                .round_robin_bindings
                .insert(role, binding)
                .is_some()
                || dispatcher.bindings.contains_key(&role)
            {
                return Err(ProductionRoleDispatchError::DuplicateRole);
            }
        }
        dispatcher.rotation_state = rotation_state;
        Ok(dispatcher)
    }

    pub fn with_session_state(mut self, session_state: SessionExecutionPolicyState) -> Self {
        self.session_state = Some(session_state);
        self
    }

    /// Creates the empty role cache used by one production turn while retaining session state.
    pub fn begin_turn(&self) -> Self {
        Self {
            bindings: self.bindings.clone(),
            round_robin_bindings: self.round_robin_bindings.clone(),
            rotation_state: Arc::clone(&self.rotation_state),
            turn_cache: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            selection_gate: Arc::new(tokio::sync::Mutex::new(())),
            session_state: self.session_state.clone(),
        }
    }

    /// Returns the immutable binding route captured for one orchestration role.
    pub fn route(
        &self,
        role: RoutingRole,
    ) -> Result<&ProductionProviderRoute, ProductionRoleDispatchError> {
        self.bindings
            .get(&role)
            .map(ProductionRoleBinding::route)
            .or_else(|| {
                self.round_robin_bindings
                    .get(&role)
                    .map(|binding| binding.route())
            })
            .ok_or(ProductionRoleDispatchError::MissingRole(role))
    }

    pub async fn prepare_role_binding(
        &self,
        role: RoutingRole,
    ) -> Result<ProductionRoleBinding, ProductionRoleDispatchError> {
        if let Some(binding) = self.bindings.get(&role) {
            return Ok(binding.clone());
        }
        let _selection_guard = self.selection_gate.lock().await;
        if let Some(binding) = self.turn_cache.lock().await.get(&role) {
            return Ok(binding.clone());
        }
        let deferred = self
            .round_robin_bindings
            .get(&role)
            .ok_or(ProductionRoleDispatchError::MissingRole(role))?;
        let generation_before = self
            .session_state
            .as_ref()
            .map(SessionExecutionPolicyState::active_generation)
            .transpose()
            .map_err(|_| ProductionRoleDispatchError::RoundRobinSelection { role })?;
        if self.session_state.is_some() && generation_before.flatten().is_none() {
            return Err(ProductionRoleDispatchError::RoundRobinSelection { role });
        }
        let mut registry = NamedAccountPoolRegistry::default();
        registry
            .insert(deferred.pool().clone())
            .map_err(|_| ProductionRoleDispatchError::RoundRobinSelection { role })?;
        let binding = {
            let mut state = self
                .rotation_state
                .lock()
                .map_err(|_| ProductionRoleDispatchError::RoundRobinSelection { role })?;
            let mut reservation = state
                .reserve_next_member(&registry, deferred.pool_id(), role)
                .map_err(|_| ProductionRoleDispatchError::RoundRobinSelection { role })?;
            let binding = deferred
                .build_for_member(reservation.member_id())
                .map_err(|_| ProductionRoleDispatchError::RoundRobinSelection { role })?;
            let generation_after = self
                .session_state
                .as_ref()
                .map(SessionExecutionPolicyState::active_generation)
                .transpose()
                .map_err(|_| ProductionRoleDispatchError::RoundRobinSelection { role })?;
            if generation_before != generation_after {
                return Err(ProductionRoleDispatchError::RoundRobinSelection { role });
            }
            reservation
                .commit(&mut state, &registry)
                .map_err(|_| ProductionRoleDispatchError::RoundRobinSelection { role })?;
            binding
        };
        self.turn_cache.lock().await.insert(role, binding.clone());
        Ok(binding)
    }

    async fn binding_for_role(
        &self,
        role: RoutingRole,
    ) -> Result<ProductionRoleBinding, ProductionRoleDispatchError> {
        self.prepare_role_binding(role).await
    }

    pub async fn invoke(
        &self,
        invocation: ProductionRoleInvocationRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProductionRoleDispatchError> {
        let role = invocation.role;
        let binding = self.binding_for_role(role).await?;
        let route = binding.route.selection();
        if invocation.request.provider != route.provider_id {
            return Err(ProductionRoleDispatchError::ProviderMismatch { role });
        }
        if invocation.connection_id != route.connection_id {
            return Err(ProductionRoleDispatchError::ConnectionMismatch { role });
        }
        if invocation.account_id.as_deref().is_some_and(|account_id| {
            route.provider_id == super::codex_invocation::CODEX_PROVIDER_ID
                && account_id != route.connection_id
        }) {
            return Err(ProductionRoleDispatchError::AccountMismatch { role });
        }
        if invocation.request.model != route.model_id {
            return Err(ProductionRoleDispatchError::ModelMismatch { role });
        }
        if invocation.effort != binding.route.effort() {
            return Err(ProductionRoleDispatchError::EffortMismatch { role });
        }
        let result = (binding.invoke)(invocation.request, cancellation)
            .await
            .map_err(|source| ProductionRoleDispatchError::ProviderFailure { role, source })?;
        if result.provider != route.provider_id || result.model != route.model_id {
            return Err(ProductionRoleDispatchError::ResultRouteMismatch { role });
        }
        Ok(result)
    }
}

impl SubagentProvider for ProductionRoleDispatcher {
    fn invoke(
        &self,
        _request: ProviderInvocationRequest,
        _cancellation: CancellationToken,
    ) -> impl Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send
    {
        async { Err(ProviderInvocationError::InvalidRequest) }
    }

    fn invoke_role(
        &self,
        role: RoutingRole,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send
    {
        async move {
            let binding = self
                .binding_for_role(role)
                .await
                .map_err(|_| ProviderInvocationError::InvalidConfiguration)?;
            let invocation =
                ProductionRoleInvocationRequest::new(role, binding.route(), request, None);
            self.invoke(invocation, cancellation)
                .await
                .map_err(|_| ProviderInvocationError::InvalidRequest)
        }
    }
}
