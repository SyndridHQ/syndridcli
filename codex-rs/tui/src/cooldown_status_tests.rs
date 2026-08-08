use super::TuiCooldownStatus;
use super::TuiProviderCooldownSnapshot;
use super::cooldown_label;
use super::failure_class_label;
use super::format_cooldown_duration;
use crate::legacy_core::AccountPoolTarget;
use crate::legacy_core::CodexAccountProfileId;
use crate::legacy_core::ProviderCooldownError;
use crate::legacy_core::ProviderCooldownKey;
use crate::legacy_core::ProviderCooldownState;
use crate::legacy_core::ProviderFailureClass;
use crate::legacy_core::SessionExecutionPolicyState;
use pretty_assertions::assert_eq;
use std::time::Duration;
use std::time::Instant;

fn account_target(id: &str) -> AccountPoolTarget {
    AccountPoolTarget::native_codex(CodexAccountProfileId::new(id).unwrap())
}

#[test]
fn snapshot_is_owned_and_reports_expiration_without_mutating_state() {
    let mut state = ProviderCooldownState::new();
    let target = account_target("account-a");
    let key = ProviderCooldownKey::new(target.clone());
    let now = Instant::now();
    state
        .record_cooldown(
            key,
            ProviderFailureClass::RateLimited,
            Duration::from_secs(42),
            now,
        )
        .unwrap();

    let snapshot = TuiProviderCooldownSnapshot::from_state(&state, now);
    assert_eq!(
        snapshot.status_for_target(&target),
        TuiCooldownStatus::CoolingDown {
            remaining: Duration::from_secs(42),
            failure_class: ProviderFailureClass::RateLimited,
        }
    );
    assert_eq!(state.len(), 1);

    let expired = TuiProviderCooldownSnapshot::from_state(&state, now + Duration::from_secs(42));
    assert_eq!(
        expired.status_for_target(&target),
        TuiCooldownStatus::Available
    );
    assert_eq!(state.len(), 1);
}

#[test]
fn snapshot_is_coherent_before_at_and_after_expiration() {
    let mut state = ProviderCooldownState::new();
    let target = account_target("account-a");
    let now = Instant::now();
    state
        .record_cooldown(
            ProviderCooldownKey::new(target.clone()),
            ProviderFailureClass::Timeout,
            Duration::from_secs(10),
            now,
        )
        .unwrap();

    assert!(matches!(
        TuiProviderCooldownSnapshot::from_state(&state, now - Duration::from_secs(1))
            .status_for_target(&target),
        TuiCooldownStatus::CoolingDown {
            remaining,
            ..
        } if remaining == Duration::from_secs(11)
    ));
    assert_eq!(
        TuiProviderCooldownSnapshot::from_state(&state, now + Duration::from_secs(10))
            .status_for_target(&target),
        TuiCooldownStatus::Available
    );
    assert_eq!(
        TuiProviderCooldownSnapshot::from_state(&state, now + Duration::from_secs(11))
            .status_for_target(&target),
        TuiCooldownStatus::Available
    );
}

#[test]
fn snapshots_are_isolated_to_their_session_policy_state() {
    let first = SessionExecutionPolicyState::new().expect("first policy state");
    let second = SessionExecutionPolicyState::new().expect("second policy state");
    let target = account_target("account-a");
    let now = Instant::now();
    first
        .cooldown_state()
        .lock()
        .expect("first cooldown lock")
        .record_cooldown(
            ProviderCooldownKey::new(target.clone()),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(20),
            now,
        )
        .unwrap();

    assert!(matches!(
        TuiProviderCooldownSnapshot::from_policy_state(&first).status_for_target(&target),
        TuiCooldownStatus::CoolingDown { .. }
    ));
    assert_eq!(
        TuiProviderCooldownSnapshot::from_policy_state(&second).status_for_target(&target),
        TuiCooldownStatus::Available
    );
}

#[test]
fn exact_targets_share_status_and_duplicate_members_are_counted_once() {
    let mut state = ProviderCooldownState::new();
    let cooling = account_target("account-a");
    let available = account_target("account-b");
    let now = Instant::now();
    state
        .record_cooldown(
            ProviderCooldownKey::new(cooling.clone()),
            ProviderFailureClass::Timeout,
            Duration::from_secs(8),
            now,
        )
        .unwrap();
    let snapshot = TuiProviderCooldownSnapshot::from_state(&state, now);
    let targets = [&cooling, &cooling, &available];
    assert_eq!(snapshot.cooling_target_count(targets), 1);
    assert_eq!(snapshot.available_target_count(targets), 1);
    assert_eq!(
        snapshot.earliest_recovery_for_targets(targets),
        Some(Duration::from_secs(8))
    );
}

#[test]
fn duration_formatting_is_compact_and_bounded() {
    assert_eq!(format_cooldown_duration(Duration::from_secs(1)), "1s");
    assert_eq!(format_cooldown_duration(Duration::from_secs(59)), "59s");
    assert_eq!(format_cooldown_duration(Duration::from_secs(60)), "1m");
    assert_eq!(format_cooldown_duration(Duration::from_secs(65)), "1m 05s");
    assert_eq!(
        format_cooldown_duration(Duration::from_secs(59 * 60)),
        "59m"
    );
    assert_eq!(format_cooldown_duration(Duration::from_secs(60 * 60)), "1h");
    assert_eq!(
        format_cooldown_duration(Duration::from_secs(60 * 60 + 5 * 60)),
        "1h 05m"
    );
    assert_eq!(
        format_cooldown_duration(Duration::from_secs(24 * 60 * 60)),
        "24h"
    );
}

#[test]
fn failure_labels_are_safe_and_bounded() {
    assert_eq!(
        failure_class_label(ProviderFailureClass::RateLimited),
        "Rate limited"
    );
    assert_eq!(
        failure_class_label(ProviderFailureClass::QuotaExhausted),
        "Usage limit reached"
    );
    assert_eq!(
        failure_class_label(ProviderFailureClass::Unknown),
        "Temporarily unavailable"
    );
    assert_eq!(
        cooldown_label(&TuiCooldownStatus::CoolingDown {
            remaining: Duration::from_secs(42),
            failure_class: ProviderFailureClass::ProviderUnavailable,
        }),
        "Cooling down · 42s · Provider temporarily unavailable"
    );
    assert_eq!(cooldown_label(&TuiCooldownStatus::Available), "Available");
}

#[test]
fn required_failure_classes_have_safe_ui_labels() {
    for (class, expected) in [
        (ProviderFailureClass::RateLimited, "Rate limited"),
        (ProviderFailureClass::QuotaExhausted, "Usage limit reached"),
        (
            ProviderFailureClass::ProviderUnavailable,
            "Provider temporarily unavailable",
        ),
        (
            ProviderFailureClass::Network,
            "Network/provider connection issue",
        ),
        (ProviderFailureClass::Timeout, "Provider timeout"),
        (ProviderFailureClass::Internal, "Temporarily unavailable"),
        (ProviderFailureClass::Unknown, "Temporarily unavailable"),
    ] {
        assert_eq!(failure_class_label(class), expected);
    }
}

#[test]
fn invalid_recording_is_not_created_by_presentation() {
    let mut state = ProviderCooldownState::new();
    let target = account_target("account-a");
    assert_eq!(
        state.record_cooldown(
            ProviderCooldownKey::new(target),
            ProviderFailureClass::Unknown,
            Duration::ZERO,
            Instant::now(),
        ),
        Err(ProviderCooldownError::ZeroDuration)
    );
    assert!(state.is_empty());
}
