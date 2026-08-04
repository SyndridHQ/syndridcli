//! Session-owned transactional state for deterministic account-pool rotation.
//!
//! This module deliberately does not resolve provider readiness or install runtimes. It reserves
//! configured members only; the caller commits a reservation after its own preparation succeeds.

use super::account_pools::AccountPoolError;
use super::account_pools::AccountPoolMember;
use super::account_pools::AccountPoolSelectionPolicy;
use super::account_pools::AccountPoolTarget;
use super::account_pools::NamedAccountPool;
use super::account_pools::NamedAccountPoolRegistry;
use super::account_pools::PoolId;
use super::account_pools::PoolMemberId;
use super::routing_profiles::RoutingRole;
use sha2::Digest;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PoolRotationKey {
    pub pool_id: PoolId,
    pub role: RoutingRole,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolRotationFingerprint([u8; 32]);

impl PoolRotationFingerprint {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PoolSelectionReservation {
    key: PoolRotationKey,
    member: AccountPoolMember,
    fingerprint: PoolRotationFingerprint,
    generation: u64,
    committed: bool,
}

impl PoolSelectionReservation {
    pub fn key(&self) -> &PoolRotationKey {
        &self.key
    }

    pub fn member(&self) -> &AccountPoolMember {
        &self.member
    }

    pub fn member_id(&self) -> &PoolMemberId {
        &self.member.id
    }

    pub fn fingerprint(&self) -> &PoolRotationFingerprint {
        &self.fingerprint
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Aborts this reservation. Dropping or aborting a reservation never changes the cursor.
    pub fn abort(self) {}

    /// Commits this reservation exactly once after the caller has completed preparation.
    pub fn commit(
        &mut self,
        state: &mut AccountPoolRotationState,
        registry: &NamedAccountPoolRegistry,
    ) -> Result<(), PoolRotationError> {
        if self.committed {
            return Err(PoolRotationError::ReservationAlreadyCommitted);
        }
        let pool = registry
            .get(&self.key.pool_id)
            .ok_or(PoolRotationError::PoolNotFound)?;
        pool.validate_structure()
            .map_err(PoolRotationError::InvalidPool)?;
        if !matches!(
            pool.selection_policy,
            AccountPoolSelectionPolicy::RoundRobin
        ) {
            return Err(PoolRotationError::UnsupportedPolicy);
        }
        let fingerprint = pool.rotation_fingerprint();
        if fingerprint != self.fingerprint {
            return Err(PoolRotationError::PoolFingerprintMismatch);
        }
        let entry = state
            .cursors
            .get_mut(&self.key)
            .ok_or(PoolRotationError::StaleReservation)?;
        if entry.generation != self.generation || entry.fingerprint != self.fingerprint {
            return Err(PoolRotationError::StaleReservation);
        }
        let ordered = canonical_members(pool);
        let Some(current) = ordered.get(entry.next_index) else {
            return Err(PoolRotationError::InvalidPool(AccountPoolError::EmptyPool));
        };
        if current.id != self.member.id || current.target != self.member.target {
            return Err(PoolRotationError::MemberNoLongerInPool);
        }
        entry.next_index = (entry.next_index + 1) % ordered.len();
        entry.generation += 1;
        self.committed = true;
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AccountPoolRotationState {
    cursors: BTreeMap<PoolRotationKey, RotationCursor>,
}

impl AccountPoolRotationState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reserve_next_member(
        &mut self,
        registry: &NamedAccountPoolRegistry,
        pool_id: &PoolId,
        role: RoutingRole,
    ) -> Result<PoolSelectionReservation, PoolRotationError> {
        let pool = registry
            .get(pool_id)
            .ok_or(PoolRotationError::PoolNotFound)?;
        pool.validate_structure()
            .map_err(PoolRotationError::InvalidPool)?;
        if !matches!(
            pool.selection_policy,
            AccountPoolSelectionPolicy::RoundRobin
        ) {
            return Err(PoolRotationError::UnsupportedPolicy);
        }
        let ordered = canonical_members(pool);
        let fingerprint = pool.rotation_fingerprint();
        let key = PoolRotationKey {
            pool_id: pool_id.clone(),
            role,
        };
        let cursor = self
            .cursors
            .entry(key.clone())
            .or_insert_with(|| RotationCursor {
                fingerprint: fingerprint.clone(),
                next_index: 0,
                generation: 0,
            });
        if cursor.fingerprint != fingerprint {
            cursor.fingerprint = fingerprint.clone();
            cursor.next_index = 0;
            cursor.generation += 1;
        }
        let member = ordered[cursor.next_index].clone();
        Ok(PoolSelectionReservation {
            key,
            member,
            fingerprint,
            generation: cursor.generation,
            committed: false,
        })
    }

    pub fn cursor_generation(&self, pool_id: &PoolId, role: RoutingRole) -> Option<u64> {
        self.cursors
            .get(&PoolRotationKey {
                pool_id: pool_id.clone(),
                role,
            })
            .map(|cursor| cursor.generation)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RotationCursor {
    fingerprint: PoolRotationFingerprint,
    next_index: usize,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoolRotationError {
    PoolNotFound,
    InvalidPool(AccountPoolError),
    UnsupportedPolicy,
    ReservationAlreadyCommitted,
    StaleReservation,
    PoolFingerprintMismatch,
    MemberNoLongerInPool,
}

impl fmt::Display for PoolRotationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PoolNotFound => "account pool was not found",
            Self::InvalidPool(_) => "account pool is structurally invalid",
            Self::UnsupportedPolicy => "account pool policy does not support runtime reservation",
            Self::ReservationAlreadyCommitted => "account pool reservation was already committed",
            Self::StaleReservation => "account pool reservation is stale",
            Self::PoolFingerprintMismatch => "account pool changed since reservation",
            Self::MemberNoLongerInPool => "reserved account pool member is no longer in the pool",
        })
    }
}

impl std::error::Error for PoolRotationError {}

impl NamedAccountPool {
    pub fn canonical_members(&self) -> Vec<&AccountPoolMember> {
        canonical_members(self)
    }

    pub fn rotation_fingerprint(&self) -> PoolRotationFingerprint {
        let mut hasher = Sha256::new();
        hash_bytes(&mut hasher, self.id.as_str().as_bytes());
        hash_bytes(
            &mut hasher,
            match self.provider_family {
                super::account_pools::AccountPoolProviderFamily::NativeCodex => b"native_codex",
                super::account_pools::AccountPoolProviderFamily::OmniRoute => b"omniroute",
            },
        );
        match &self.selection_policy {
            AccountPoolSelectionPolicy::ExplicitMember(member_id) => {
                hash_bytes(&mut hasher, b"explicit_member");
                hash_bytes(&mut hasher, member_id.as_str().as_bytes());
            }
            AccountPoolSelectionPolicy::RoundRobin => hash_bytes(&mut hasher, b"round_robin"),
        }
        for member in canonical_members(self) {
            hash_bytes(&mut hasher, member.id.as_str().as_bytes());
            match &member.target {
                AccountPoolTarget::NativeCodexAccount(account_id) => {
                    hash_bytes(&mut hasher, b"native_codex_account");
                    hash_bytes(&mut hasher, account_id.as_str().as_bytes());
                }
                AccountPoolTarget::OmniRouteConnection(connection_id) => {
                    hash_bytes(&mut hasher, b"omniroute_connection");
                    hash_bytes(&mut hasher, connection_id.as_bytes());
                }
            }
        }
        PoolRotationFingerprint(hasher.finalize().into())
    }
}

fn canonical_members(pool: &NamedAccountPool) -> Vec<&AccountPoolMember> {
    let mut members = pool.members.iter().collect::<Vec<_>>();
    members.sort_by(|left, right| left.id.cmp(&right.id));
    members
}

fn hash_bytes(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u32).to_be_bytes());
    hasher.update(value);
}

#[cfg(test)]
#[path = "rotation_state_tests.rs"]
mod rotation_state_tests;
