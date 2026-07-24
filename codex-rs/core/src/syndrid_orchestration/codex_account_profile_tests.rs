use super::*;

fn metadata(
    id: &str,
    connection_id: &str,
    credential_reference: &str,
) -> CodexAccountConnectionMetadata {
    CodexAccountConnectionMetadata {
        connection_id: connection_id.to_string(),
        profile_id: CodexAccountProfileId::new(id).expect("profile id"),
        provider_id: "codex".to_string(),
        label: id.to_string(),
        state: CodexAccountProfileState::Connected,
        account_email: Some("account-email-sentinel".to_string()),
        account_id: Some("account-id-sentinel".to_string()),
        plan_label: Some("plan".to_string()),
        enabled: true,
        validation:
            crate::syndrid_orchestration::provider_connection::ConnectionValidationStatus::Valid,
        last_authenticated_at: Some(1),
        last_validated_at: Some(1),
        credential_reference: credential_reference.to_string(),
    }
}

#[test]
fn separate_codex_account_metadata_records_are_isolated() {
    let mut registry = CodexAccountProfileRegistry::default();
    registry
        .insert(metadata(
            "codex-personal",
            "codex-personal",
            "cred-personal",
        ))
        .expect("personal");
    registry
        .insert(metadata("codex-work", "codex-work", "cred-work"))
        .expect("work");
    assert_eq!(registry.profiles().count(), 2);
    assert_eq!(
        registry
            .get(&CodexAccountProfileId::new("codex-work").expect("id"))
            .expect("work")
            .credential_reference,
        "cred-work"
    );
    assert_eq!(
        registry.insert(metadata("codex-work", "codex-other", "cred-other")),
        Err(CodexAccountProfileError::DuplicateCodexAccountProfile)
    );
}

#[test]
fn account_metadata_debug_redacts_identity_and_credentials() {
    let item = metadata("codex-personal", "codex-personal", "credential-sentinel");
    let debug = format!("{item:?}");
    assert!(!debug.contains("account-email-sentinel"));
    assert!(!debug.contains("account-id-sentinel"));
    assert!(!debug.contains("credential-sentinel"));
}

#[test]
fn invalid_account_state_and_id_are_rejected() {
    assert_eq!(
        CodexAccountProfileId::new("../account"),
        Err(CodexAccountProfileError::InvalidCodexAccountProfileId)
    );
    let mut item = metadata("codex-personal", "codex-personal", "cred");
    item.provider_id = "openrouter".to_string();
    let mut registry = CodexAccountProfileRegistry::default();
    assert_eq!(
        registry.insert(item),
        Err(CodexAccountProfileError::InvalidAccountState)
    );
}
