use super::AccountPoolTarget;
use super::ProviderCooldownError;
use super::ProviderCooldownKey;
use super::ProviderCooldownState;
use super::ProviderCooldownStatus;
use super::ProviderFailureClass;
use crate::CodexAccountProfileId;
use crate::SessionExecutionPolicyState;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

fn account_key(id: &str) -> ProviderCooldownKey {
    ProviderCooldownKey::new(AccountPoolTarget::native_codex(
        CodexAccountProfileId::new(id).unwrap(),
    ))
}

fn connection_key(id: &str) -> ProviderCooldownKey {
    ProviderCooldownKey::new(AccountPoolTarget::omniroute(id).unwrap())
}

#[test]
fn exact_targets_share_across_pools_and_roles_but_not_with_other_targets() {
    assert_eq!(account_key("account-a"), account_key("account-a"));
    assert_ne!(account_key("account-a"), account_key("account-b"));
    assert_ne!(account_key("account-a"), connection_key("account-a"));
}

#[test]
fn recording_extends_without_shortening_and_replaces_expired_entries() {
    let mut state = ProviderCooldownState::new();
    let key = account_key("account-a");
    let now = Instant::now();
    state
        .record_cooldown(
            key.clone(),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(60),
            now,
        )
        .unwrap();
    state
        .record_cooldown(
            key.clone(),
            ProviderFailureClass::ProviderUnavailable,
            Duration::from_secs(30),
            now,
        )
        .unwrap();
    assert_eq!(
        state.status(&key, now + Duration::from_secs(30)),
        ProviderCooldownStatus::CoolingDown {
            remaining: Duration::from_secs(30),
            failure_class: ProviderFailureClass::RateLimited,
        }
    );
    state
        .record_cooldown(
            key.clone(),
            ProviderFailureClass::ProviderUnavailable,
            Duration::from_secs(120),
            now,
        )
        .unwrap();
    assert_eq!(
        state.status(&key, now + Duration::from_secs(60)),
        ProviderCooldownStatus::CoolingDown {
            remaining: Duration::from_secs(60),
            failure_class: ProviderFailureClass::ProviderUnavailable,
        }
    );
    state
        .record_cooldown(
            key.clone(),
            ProviderFailureClass::Authentication,
            Duration::from_secs(10),
            now + Duration::from_secs(121),
        )
        .unwrap();
    assert_eq!(
        state.status(&key, now + Duration::from_secs(121)),
        ProviderCooldownStatus::CoolingDown {
            remaining: Duration::from_secs(10),
            failure_class: ProviderFailureClass::Authentication,
        }
    );
}

#[test]
fn status_is_available_at_expiration_and_lazily_cleans_up() {
    let mut state = ProviderCooldownState::new();
    let key = account_key("account-a");
    let now = Instant::now();
    state
        .record_cooldown(
            key.clone(),
            ProviderFailureClass::Timeout,
            Duration::from_secs(5),
            now,
        )
        .unwrap();
    assert_eq!(
        state.status(&key, now + Duration::from_secs(4)),
        ProviderCooldownStatus::CoolingDown {
            remaining: Duration::from_secs(1),
            failure_class: ProviderFailureClass::Timeout,
        }
    );
    assert_eq!(
        state.status(&key, now + Duration::from_secs(5)),
        ProviderCooldownStatus::Available
    );
    assert!(state.is_empty());
}

#[test]
fn explicit_prune_removes_only_expired_records() {
    let mut state = ProviderCooldownState::new();
    let now = Instant::now();
    let expired = account_key("expired");
    let active = account_key("active");
    state
        .record_cooldown(
            expired,
            ProviderFailureClass::Network,
            Duration::from_secs(1),
            now,
        )
        .unwrap();
    state
        .record_cooldown(
            active.clone(),
            ProviderFailureClass::Network,
            Duration::from_secs(10),
            now,
        )
        .unwrap();
    assert_eq!(state.prune_expired(now + Duration::from_secs(1)), 1);
    assert_eq!(state.len(), 1);
    assert!(matches!(
        state.status(&active, now + Duration::from_secs(1)),
        ProviderCooldownStatus::CoolingDown { .. }
    ));
}

#[test]
fn invalid_durations_have_no_state_effect() {
    let mut state = ProviderCooldownState::new();
    let key = account_key("account-a");
    let now = Instant::now();
    assert_eq!(
        state.record_cooldown(
            key.clone(),
            ProviderFailureClass::Unknown,
            Duration::ZERO,
            now
        ),
        Err(ProviderCooldownError::ZeroDuration)
    );
    assert_eq!(
        state.record_cooldown(
            key.clone(),
            ProviderFailureClass::Unknown,
            super::MAX_PROVIDER_COOLDOWN + Duration::from_secs(1),
            now,
        ),
        Err(ProviderCooldownError::DurationExceedsMaximum)
    );
    assert_eq!(state.status(&key, now), ProviderCooldownStatus::Available);
}

#[test]
fn owned_session_handles_share_cooldown_state_without_global_state() {
    let first = Arc::new(Mutex::new(ProviderCooldownState::new()));
    let second = Arc::new(Mutex::new(ProviderCooldownState::new()));
    let key = account_key("account-a");
    let now = Instant::now();
    first
        .lock()
        .unwrap()
        .record_cooldown(
            key.clone(),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(1),
            now,
        )
        .unwrap();
    assert!(matches!(
        first.lock().unwrap().status(&key, now),
        ProviderCooldownStatus::CoolingDown { .. }
    ));
    assert_eq!(
        second.lock().unwrap().status(&key, now),
        ProviderCooldownStatus::Available
    );
}

#[test]
fn session_authority_owns_cooldowns_across_turns_without_sharing_sessions() {
    let first_session = SessionExecutionPolicyState::new().unwrap();
    let second_session = SessionExecutionPolicyState::new().unwrap();
    let first_cooldowns = first_session.cooldown_state();
    let first_again = first_session.cooldown_state();
    assert!(Arc::ptr_eq(&first_cooldowns, &first_again));
    assert!(!Arc::ptr_eq(
        &first_cooldowns,
        &second_session.cooldown_state()
    ));

    let key = account_key("account-a");
    let now = Instant::now();
    first_cooldowns
        .lock()
        .unwrap()
        .record_cooldown(
            key.clone(),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(30),
            now,
        )
        .unwrap();
    assert!(matches!(
        first_session
            .cooldown_state()
            .lock()
            .unwrap()
            .status(&key, now),
        ProviderCooldownStatus::CoolingDown { .. }
    ));
    assert_eq!(
        second_session
            .cooldown_state()
            .lock()
            .unwrap()
            .status(&key, now),
        ProviderCooldownStatus::Available
    );
}

#[test]
fn session_lock_keeps_the_longer_concurrent_update() {
    let state = Arc::new(Mutex::new(ProviderCooldownState::new()));
    let key = account_key("account-a");
    let now = Instant::now();
    {
        let mut guard = state.lock().unwrap();
        guard
            .record_cooldown(
                key.clone(),
                ProviderFailureClass::RateLimited,
                Duration::from_secs(30),
                now,
            )
            .unwrap();
    }
    {
        let mut guard = state.lock().unwrap();
        guard
            .record_cooldown(
                key.clone(),
                ProviderFailureClass::ProviderUnavailable,
                Duration::from_secs(60),
                now,
            )
            .unwrap();
    }
    {
        let mut guard = state.lock().unwrap();
        guard
            .record_cooldown(
                key.clone(),
                ProviderFailureClass::Authentication,
                Duration::from_secs(10),
                now,
            )
            .unwrap();
    }
    assert_eq!(
        state
            .lock()
            .unwrap()
            .status(&key, now + Duration::from_secs(30)),
        ProviderCooldownStatus::CoolingDown {
            remaining: Duration::from_secs(30),
            failure_class: ProviderFailureClass::ProviderUnavailable,
        }
    );
}
