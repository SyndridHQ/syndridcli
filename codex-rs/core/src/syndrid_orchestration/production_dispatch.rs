use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use super::production_request::ProductionProviderRoute;
use super::routing_profiles::RoutingRole;
use super::subagent::SubagentProvider;
use codex_protocol::openai_models::ReasoningEffort;
use std::collections::BTreeMap;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
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
}

/// Dispatches each orchestration role through its immutable, exact provider route.
///
/// This type performs route validation and delegation only. It does not resolve profiles or
/// policy, create tools, own cancellation, publish observations, or translate final results.
#[derive(Clone, Debug, Default)]
pub struct ProductionRoleDispatcher {
    bindings: BTreeMap<RoutingRole, ProductionRoleBinding>,
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
        Ok(Self { bindings: resolved })
    }

    pub async fn invoke(
        &self,
        invocation: ProductionRoleInvocationRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProductionRoleDispatchError> {
        let role = invocation.role;
        let binding = self
            .bindings
            .get(&role)
            .ok_or(ProductionRoleDispatchError::MissingRole(role))?;
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
