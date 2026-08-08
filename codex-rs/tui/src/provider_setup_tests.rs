use super::ProviderSetupSnapshot;
use crate::cooldown_status::TuiProviderCooldownSnapshot;
use crate::legacy_core::AccountPoolTarget;
use crate::legacy_core::CodexAccountProfileId;
use crate::legacy_core::ProviderCooldownKey;
use crate::legacy_core::ProviderCooldownState;
use crate::legacy_core::ProviderFailureClass;
use crate::orchestration_setup::SetupReadinessState;
use crate::provider_setup::ProviderSetupItem;
use crate::syndrid_composition::TuiCanonicalAuthorities;
use pretty_assertions::assert_eq;
use std::time::Duration;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn empty_authorities_report_bounded_provider_readiness() {
    let home = tempdir().expect("temporary Codex home");
    let authorities = TuiCanonicalAuthorities::load(home.path());
    let snapshot = ProviderSetupSnapshot::from_authorities(&authorities);

    assert_eq!(
        snapshot
            .providers
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Native Codex", "OmniRoute", "OpenRouter"]
    );
    assert_eq!(snapshot.accounts, Vec::new());
    assert_eq!(snapshot.connections, Vec::new());
    assert!(
        snapshot.providers[2]
            .readiness
            .reason()
            .expect("OpenRouter reason")
            .contains("not implemented")
    );
}

#[test]
fn unavailable_snapshot_does_not_include_secret_bearing_metadata() {
    let snapshot = ProviderSetupSnapshot::unavailable();
    let debug = format!("{snapshot:?}");
    assert!(!debug.contains("token"));
    assert!(!debug.contains("credential"));
    assert!(debug.contains("OpenRouter"));
}

#[test]
fn account_setup_detail_uses_the_exact_cooldown_target_without_mutation() {
    let target = AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-a").unwrap());
    let now = Instant::now();
    let mut state = ProviderCooldownState::new();
    state
        .record_cooldown(
            ProviderCooldownKey::new(target.clone()),
            ProviderFailureClass::RateLimited,
            Duration::from_secs(42),
            now,
        )
        .unwrap();
    let snapshot = ProviderSetupSnapshot {
        accounts: vec![ProviderSetupItem {
            name: "Account A".to_string(),
            detail: "Native Codex account".to_string(),
            id: Some("account-a".to_string()),
            provider_id: Some("codex".to_string()),
            target: Some(target),
            models: Vec::new(),
            readiness: SetupReadinessState::Ready,
        }],
        ..Default::default()
    };
    let rendered = snapshot.with_cooldowns(&TuiProviderCooldownSnapshot::from_state(&state, now));
    assert_eq!(
        rendered.accounts[0].detail,
        "Native Codex account · Cooling down · 42s · Rate limited"
    );
    assert_eq!(state.len(), 1);
}

#[test]
fn connection_setup_detail_uses_the_exact_cooldown_target_without_mutation() {
    let target = AccountPoolTarget::omniroute("connection-a").unwrap();
    let now = Instant::now();
    let mut state = ProviderCooldownState::new();
    state
        .record_cooldown(
            ProviderCooldownKey::new(target.clone()),
            ProviderFailureClass::ProviderUnavailable,
            Duration::from_secs(18),
            now,
        )
        .unwrap();
    let snapshot = ProviderSetupSnapshot {
        connections: vec![ProviderSetupItem {
            name: "Connection A".to_string(),
            detail: "OmniRoute · 1 configured models".to_string(),
            id: Some("connection-a".to_string()),
            provider_id: Some("omniroute".to_string()),
            target: Some(target),
            models: vec!["model-a".to_string()],
            readiness: SetupReadinessState::Ready,
        }],
        ..Default::default()
    };
    let rendered = snapshot.with_cooldowns(&TuiProviderCooldownSnapshot::from_state(&state, now));
    assert_eq!(
        rendered.connections[0].detail,
        "OmniRoute · 1 configured models · Cooling down · 18s · Provider temporarily unavailable"
    );
    assert_eq!(state.len(), 1);
}

#[test]
fn readiness_and_cooldown_are_separate_for_accounts_and_connections() {
    fn item(
        name: &str,
        detail: &str,
        id: &str,
        provider_id: &str,
        target: AccountPoolTarget,
        readiness: SetupReadinessState,
    ) -> ProviderSetupItem {
        ProviderSetupItem {
            name: name.to_string(),
            detail: detail.to_string(),
            id: Some(id.to_string()),
            provider_id: Some(provider_id.to_string()),
            target: Some(target),
            models: if provider_id == "omniroute" {
                vec!["model-a".to_string()]
            } else {
                Vec::new()
            },
            readiness,
        }
    }

    let account_target =
        AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-a").unwrap());
    let connection_target = AccountPoolTarget::omniroute("connection-a").unwrap();
    let now = Instant::now();
    let mut state = ProviderCooldownState::new();
    for target in [&account_target, &connection_target] {
        state
            .record_cooldown(
                ProviderCooldownKey::new(target.clone()),
                ProviderFailureClass::RateLimited,
                Duration::from_secs(24),
                now,
            )
            .unwrap();
    }
    let snapshot = ProviderSetupSnapshot {
        accounts: vec![
            item(
                "Ready account",
                "Native Codex account",
                "account-a",
                "codex",
                account_target,
                SetupReadinessState::Ready,
            ),
            item(
                "Needs attention account",
                "Native Codex account",
                "account-b",
                "codex",
                AccountPoolTarget::native_codex(CodexAccountProfileId::new("account-b").unwrap()),
                SetupReadinessState::Invalid("authentication needs attention".into()),
            ),
        ],
        connections: vec![
            item(
                "Ready connection",
                "OmniRoute · 1 configured models",
                "connection-a",
                "omniroute",
                connection_target,
                SetupReadinessState::Ready,
            ),
            item(
                "Needs attention connection",
                "OmniRoute · 1 configured models",
                "connection-b",
                "omniroute",
                AccountPoolTarget::omniroute("connection-b").unwrap(),
                SetupReadinessState::Invalid("connection needs attention".into()),
            ),
        ],
        ..Default::default()
    };

    let rendered = snapshot.with_cooldowns(&TuiProviderCooldownSnapshot::from_state(&state, now));
    assert_eq!(rendered.accounts[0].readiness, SetupReadinessState::Ready);
    assert!(rendered.accounts[0].detail.contains("Cooling down · 24s"));
    assert!(matches!(
        rendered.accounts[1].readiness,
        SetupReadinessState::Invalid(_)
    ));
    assert!(rendered.accounts[1].detail.ends_with("Available"));
    assert_eq!(
        rendered.connections[0].readiness,
        SetupReadinessState::Ready
    );
    assert!(
        rendered.connections[0]
            .detail
            .contains("Cooling down · 24s")
    );
    assert!(matches!(
        rendered.connections[1].readiness,
        SetupReadinessState::Invalid(_)
    ));
    assert!(rendered.connections[1].detail.ends_with("Available"));
}

#[test]
fn provider_setup_summary_snapshot_is_redacted_and_bounded() {
    let home = tempdir().expect("temporary Codex home");
    let authorities = TuiCanonicalAuthorities::load(home.path());
    let snapshot = ProviderSetupSnapshot::from_authorities(&authorities);
    let summary = snapshot
        .providers
        .iter()
        .map(|item| {
            format!(
                "{} — {} — {}",
                item.name,
                item.detail,
                item.readiness.label()
            )
        })
        .chain(
            snapshot
                .accounts
                .iter()
                .map(|item| format!("account: {} — {}", item.name, item.readiness.label())),
        )
        .chain(
            snapshot
                .connections
                .iter()
                .map(|item| format!("connection: {} — {}", item.name, item.readiness.label())),
        )
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(summary);
}
