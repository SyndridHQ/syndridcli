use super::*;
use codex_login::AuthDotJson;
use codex_login::TokenData;
use codex_protocol::auth::AuthMode;

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
        schema_version: 1,
    }
}

#[test]
fn separate_codex_account_metadata_records_are_isolated() {
    let mut registry = CodexAccountProfileRegistry::default();
    registry
        .insert(metadata(
            "codex-personal",
            "codex-personal",
            "codex-account-codex-personal",
        ))
        .expect("personal");
    registry
        .insert(metadata(
            "codex-work",
            "codex-work",
            "codex-account-codex-work",
        ))
        .expect("work");
    assert_eq!(registry.profiles().count(), 2);
    assert_eq!(
        registry
            .get(&CodexAccountProfileId::new("codex-work").expect("id"))
            .expect("work")
            .credential_reference,
        "codex-account-codex-work"
    );
    assert_eq!(
        registry.insert(metadata(
            "codex-work",
            "codex-other",
            "codex-account-codex-other",
        )),
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

#[test]
fn duplicate_credential_reference_is_rejected() {
    let mut registry = CodexAccountProfileRegistry::default();
    registry
        .insert(metadata(
            "codex-personal",
            "codex-personal",
            "codex-account-codex-personal",
        ))
        .expect("first account");
    assert_eq!(
        registry.insert(metadata(
            "codex-work",
            "codex-work",
            "codex-account-codex-personal",
        )),
        Err(CodexAccountProfileError::InvalidAccountState)
    );
}

#[test]
fn account_state_transitions_are_explicit() {
    assert!(
        CodexAccountProfileState::Unconfigured
            .can_transition_to(CodexAccountProfileState::AuthenticationPending)
    );
    assert!(
        CodexAccountProfileState::AuthenticationPending
            .can_transition_to(CodexAccountProfileState::Connected)
    );
    assert!(
        CodexAccountProfileState::Connected
            .can_transition_to(CodexAccountProfileState::ReauthenticationRequired)
    );
    assert!(
        !CodexAccountProfileState::Connected
            .can_transition_to(CodexAccountProfileState::AuthenticationPending)
    );
}

#[test]
fn account_registry_round_trips_without_identity_in_debug() {
    let directory = tempfile::tempdir().expect("temp directory");
    let path = directory.path().join("accounts.json");
    let mut registry = CodexAccountProfileRegistry::default();
    registry
        .insert(metadata(
            "codex-personal",
            "codex-personal",
            "codex-account-codex-personal",
        ))
        .expect("account");
    registry.save(&path).expect("save registry");
    let restored = CodexAccountProfileRegistry::load(&path).expect("load registry");
    assert_eq!(restored, registry);
    let serialized = std::fs::read_to_string(path).expect("serialized registry");
    assert!(!serialized.contains("access-token-sentinel"));
    let debug = format!("{restored:?}");
    assert!(!debug.contains("account-email-sentinel"));
    assert!(!debug.contains("account-id-sentinel"));
    assert!(!debug.contains("credential-sentinel"));
}

#[test]
fn credential_reference_must_match_connection_id() {
    let mut registry = CodexAccountProfileRegistry::default();
    assert_eq!(
        registry.insert(metadata("codex-personal", "codex-personal", "wrong-ref")),
        Err(CodexAccountProfileError::InvalidAccountState)
    );
}

fn oauth_auth(access: &str, refresh: &str, id_token: &str) -> AuthDotJson {
    AuthDotJson {
        auth_mode: Some(AuthMode::Chatgpt),
        openai_api_key: None,
        tokens: Some(TokenData {
            id_token: codex_login::token_data::IdTokenInfo {
                raw_jwt: id_token.to_string(),
                ..Default::default()
            },
            access_token: access.to_string(),
            refresh_token: refresh.to_string(),
            account_id: Some("account-id".to_string()),
        }),
        last_refresh: None,
        agent_identity: None,
        personal_access_token: None,
        bedrock_api_key: None,
    }
}

#[test]
fn credential_envelope_round_trips_and_redacts_tokens() {
    let envelope = CodexCredentialEnvelope::from_auth(&oauth_auth(
        "access-token-sentinel",
        "refresh-token-sentinel",
        "eyJhbGciOiJub25lIn0.eyJzdWIiOiIxIn0.sig",
    ))
    .expect("envelope");
    let serialized = envelope.serialized().expect("serialized envelope");
    let restored = CodexCredentialEnvelope::parse(&serialized).expect("restored envelope");
    assert_eq!(restored, envelope);
    let debug = format!("{restored:?}");
    let display = restored.to_string();
    assert!(!debug.contains("access-token-sentinel"));
    assert!(!debug.contains("refresh-token-sentinel"));
    assert!(!debug.contains("eyJhbGciOiJub25lIn0"));
    assert!(!display.contains("access-token-sentinel"));
}

#[test]
fn credential_envelope_rejects_bad_versions_and_required_fields() {
    let serialized = CodexCredentialEnvelope::from_auth(&oauth_auth(
        "a",
        "r",
        "eyJhbGciOiJub25lIn0.eyJzdWIiOiIxIn0.sig",
    ))
    .expect("envelope")
    .serialized()
    .expect("serialized");
    let mut value: serde_json::Value = serde_json::from_str(&serialized).expect("json");
    value["schema_version"] = serde_json::json!(99);
    assert_eq!(
        CodexCredentialEnvelope::parse(&value.to_string()),
        Err(CodexAccountProfileError::UnsupportedCredentialEnvelopeVersion)
    );
    assert_eq!(
        CodexCredentialEnvelope::parse("not-json"),
        Err(CodexAccountProfileError::InvalidCredentialEnvelope)
    );
    let missing_access = oauth_auth(
        "",
        "refresh-token-sentinel",
        "eyJhbGciOiJub25lIn0.eyJzdWIiOiIxIn0.sig",
    );
    assert_eq!(
        CodexCredentialEnvelope::from_auth(&missing_access),
        Err(CodexAccountProfileError::MissingRequiredCredentialField)
    );
    assert_eq!(
        CodexCredentialEnvelope::parse(&"x".repeat(64 * 1024 + 1)),
        Err(CodexAccountProfileError::CredentialEnvelopeTooLarge)
    );
    for error in [
        CodexAccountProfileError::InvalidCredentialEnvelope,
        CodexAccountProfileError::UnsupportedCredentialEnvelopeVersion,
        CodexAccountProfileError::CredentialEnvelopeTooLarge,
        CodexAccountProfileError::MissingRequiredCredentialField,
    ] {
        let display = error.to_string();
        assert!(!display.contains("access-token-sentinel"));
        assert!(!display.contains("refresh-token-sentinel"));
        assert!(!display.contains("id-token-sentinel"));
    }
}
