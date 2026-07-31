use super::codex_accounts::CodexAccountProfileRegistry;
use super::codex_invocation::CODEX_PROVIDER_ID;
use super::codex_invocation::CodexInvocationAdapter;
use super::omniroute::OmniRouteConnectionMetadata;
use super::omniroute::native_omniroute_adapter;
use super::production_dispatch::ProductionRoleBinding;
use super::production_request::ProductionProviderAdapter;
use super::production_request::ProductionProviderRoute;
use super::routing_profiles::RoutingRole;
use super::scoped_codex_session::ScopedCodexInvocationClient;
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
    fn build(&self) -> Result<ProductionRoleBinding, ProviderConstructionError> {
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

/// Immutable provider construction authorities captured for one routing snapshot.
#[derive(Clone)]
pub struct ProductionProviderConstructionSnapshot {
    bindings: BTreeMap<RoutingRole, ProductionProviderConstructionBinding>,
}

impl fmt::Debug for ProductionProviderConstructionSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductionProviderConstructionSnapshot")
            .field("role_count", &self.bindings.len())
            .field("bindings", &"<redacted>")
            .finish()
    }
}

impl ProductionProviderConstructionSnapshot {
    /// Creates an immutable snapshot from exact, already validated role bindings.
    pub fn new(bindings: BTreeMap<RoutingRole, ProductionProviderConstructionBinding>) -> Self {
        Self { bindings }
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

    /// Returns the captured roles in deterministic order.
    pub fn roles(&self) -> impl Iterator<Item = RoutingRole> + '_ {
        self.bindings.keys().copied()
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
