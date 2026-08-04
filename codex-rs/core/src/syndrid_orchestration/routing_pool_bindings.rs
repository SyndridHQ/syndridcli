//! Canonical resolution of persisted routing assignments that reference pools.

use super::account_pools::AccountPoolProviderFamily;
use super::account_pools::AccountPoolTarget;
use super::account_pools::NamedAccountPoolRegistry;
use super::account_pools::PoolId;
use super::codex_accounts::CodexAccountProfileRegistry;
use super::omniroute::OMNIROUTE_PROVIDER_ID;
use super::omniroute::OmniRouteRegistry;
use super::routing_profiles::RoutingProfile;
use super::routing_profiles::RoutingProfileError;

const ROUTING_POOL_CODEX_PROVIDER_ID: &str = "codex";

/// Resolves pool references in a source profile into exact direct identities.
///
/// The returned profile is a preparation snapshot. It contains no pool references and never
/// mutates the source profile or any registry. A pool-bound assignment is resolved only through
/// its explicit selected member; no other member is inspected as a fallback.
pub fn resolve_routing_profile(
    source_profile: &RoutingProfile,
    pools: &NamedAccountPoolRegistry,
    accounts: &CodexAccountProfileRegistry,
    connections: &OmniRouteRegistry,
) -> Result<RoutingProfile, RoutingPoolResolutionError> {
    let mut resolved = source_profile.clone();
    for (role, source_assignment) in &source_profile.assignments {
        let Some(pool_id) = &source_assignment.pool_id else {
            continue;
        };
        let provider_family = provider_family(&source_assignment.provider_id)?;
        let member = pools
            .resolve_pool(pool_id, accounts, connections)
            .map_err(|error| RoutingPoolResolutionError::Pool {
                pool_id: pool_id.clone(),
                error,
            })?;
        if member.target.provider_family() != provider_family {
            return Err(RoutingPoolResolutionError::ProviderFamilyMismatch {
                pool_id: pool_id.clone(),
            });
        }
        let connection_id = match member.target {
            AccountPoolTarget::NativeCodexAccount(account_id) => accounts
                .get(&account_id)
                .map(|account| account.connection_id.clone())
                .ok_or(RoutingPoolResolutionError::Pool {
                    pool_id: pool_id.clone(),
                    error: super::account_pools::PoolResolutionError::MissingAccountReference,
                })?,
            AccountPoolTarget::OmniRouteConnection(connection_id) => connection_id,
        };
        let mut assignment = source_assignment.clone();
        assignment.connection_id = connection_id;
        assignment.pool_id = None;
        resolved
            .replace_assignment(*role, assignment)
            .map_err(RoutingPoolResolutionError::RoutingProfile)?;
    }
    Ok(resolved)
}

fn provider_family(
    provider_id: &str,
) -> Result<AccountPoolProviderFamily, RoutingPoolResolutionError> {
    match provider_id {
        ROUTING_POOL_CODEX_PROVIDER_ID => Ok(AccountPoolProviderFamily::NativeCodex),
        OMNIROUTE_PROVIDER_ID => Ok(AccountPoolProviderFamily::OmniRoute),
        _ => Err(RoutingPoolResolutionError::UnsupportedProvider),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoutingPoolResolutionError {
    Pool {
        pool_id: PoolId,
        error: super::account_pools::PoolResolutionError,
    },
    ProviderFamilyMismatch {
        pool_id: PoolId,
    },
    UnsupportedProvider,
    RoutingProfile(RoutingProfileError),
}

impl std::fmt::Display for RoutingPoolResolutionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pool { error, .. } => write!(formatter, "pool resolution failed: {error:?}"),
            Self::ProviderFamilyMismatch { .. } => {
                formatter.write_str("routing assignment provider does not match the pool")
            }
            Self::UnsupportedProvider => {
                formatter.write_str("pool binding uses an unsupported provider")
            }
            Self::RoutingProfile(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RoutingPoolResolutionError {}
