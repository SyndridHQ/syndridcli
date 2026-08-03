use super::ProviderSetupSnapshot;
use crate::syndrid_composition::TuiCanonicalAuthorities;
use pretty_assertions::assert_eq;
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
