use super::*;
use crate::CodexAccountConnectionMetadata;
use crate::CodexAccountProfileId;
use crate::CodexAccountProfileState;
use crate::ConnectionValidationStatus;
use crate::syndrid_orchestration::omniroute::OMNIROUTE_DEFAULT_BASE_URL;
use crate::syndrid_orchestration::omniroute::OMNIROUTE_PROVIDER_ID;
use crate::syndrid_orchestration::omniroute::ProviderSelection;
use crate::syndrid_orchestration::provider_connection::ConnectionValidationResult;
use codex_protocol::openai_models::ReasoningEffort;
use pretty_assertions::assert_eq;

fn route(connection: &str, provider: &str, model: &str) -> ProductionProviderRoute {
    ProductionProviderRoute::new(
        ProviderSelection::new(connection, provider, model).expect("selection"),
        ReasoningEffort::Medium,
    )
}

fn codex_account(connection_id: &str) -> CodexAccountConnectionMetadata {
    CodexAccountConnectionMetadata {
        connection_id: connection_id.to_string(),
        profile_id: CodexAccountProfileId::new(connection_id).expect("profile id"),
        provider_id: CODEX_PROVIDER_ID.to_string(),
        label: "test account".to_string(),
        state: CodexAccountProfileState::Connected,
        account_email: None,
        account_id: Some("opaque-account".to_string()),
        plan_label: None,
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        last_authenticated_at: None,
        last_validated_at: None,
        credential_reference: CodexAccountProfileRegistry::credential_reference_for(connection_id)
            .expect("credential reference"),
        schema_version: 1,
    }
}

fn omniroute_connection() -> OmniRouteConnectionMetadata {
    OmniRouteConnectionMetadata {
        connection_id: "omni-test".to_string(),
        provider_id: OMNIROUTE_PROVIDER_ID.to_string(),
        label: "test connection".to_string(),
        base_url: OMNIROUTE_DEFAULT_BASE_URL.to_string(),
        credential_reference: "opaque-credential".to_string(),
        enabled: true,
        validation: ConnectionValidationResult::valid(),
        models: vec!["omni-model".to_string()],
        validated_at: Some(1),
    }
}

#[test]
fn native_authority_preserves_exact_route_without_invocation() {
    let connection = "codex-test";
    let route = route(connection, CODEX_PROVIDER_ID, "codex-model");
    let mut accounts = CodexAccountProfileRegistry::default();
    accounts.insert(codex_account(connection)).expect("account");

    let binding = native_codex_binding(route.clone(), accounts).expect("binding");
    assert_eq!(binding.route, route);
    binding.build().expect("deferred adapter construction");
}

#[test]
fn native_authority_rejects_missing_or_unauthenticated_exact_account() {
    let missing = native_codex_binding(
        route("codex-missing", CODEX_PROVIDER_ID, "codex-model"),
        CodexAccountProfileRegistry::default(),
    );
    assert!(matches!(
        missing,
        Err(ProviderConstructionError::AccountMissing)
    ));

    let connection = "codex-unauthenticated";
    let mut accounts = CodexAccountProfileRegistry::default();
    let mut account = codex_account(connection);
    account.enabled = false;
    accounts.insert(account).expect("account");
    let unauthenticated = native_codex_binding(
        route(connection, CODEX_PROVIDER_ID, "codex-model"),
        accounts,
    );
    assert!(matches!(
        unauthenticated,
        Err(ProviderConstructionError::AccountUnauthenticated)
    ));
}

#[test]
fn omniroute_authority_preserves_exact_connection_and_model() {
    let connection = omniroute_connection();
    let route = route("omni-test", OMNIROUTE_PROVIDER_ID, "omni-model");

    let binding = omniroute_binding(route.clone(), connection).expect("binding");
    assert_eq!(binding.route, route);
    binding.build().expect("deferred adapter construction");
}

#[test]
fn omniroute_authority_rejects_a_different_connection_or_model() {
    let connection = omniroute_connection();
    let different_connection = omniroute_binding(
        route("other", OMNIROUTE_PROVIDER_ID, "omni-model"),
        connection.clone(),
    );
    assert!(matches!(
        different_connection,
        Err(ProviderConstructionError::ConnectionMissing)
    ));
    let different_model = omniroute_binding(
        route("omni-test", OMNIROUTE_PROVIDER_ID, "other-model"),
        connection,
    );
    assert!(matches!(
        different_model,
        Err(ProviderConstructionError::ConnectionMissing)
    ));
}

#[test]
fn construction_authority_debug_redacts_route_and_connection_details() {
    let connection = omniroute_connection();
    let binding = omniroute_binding(
        route("omni-test", OMNIROUTE_PROVIDER_ID, "omni-model"),
        connection,
    )
    .expect("binding");
    let debug = format!("{binding:?}");
    assert!(!debug.contains("omni-model"));
    assert!(!debug.contains("opaque-credential"));
}

#[test]
fn openrouter_remains_unavailable_without_a_production_adapter() {
    let route = route("openrouter-test", "openrouter", "openrouter-model");
    let binding = openrouter_binding(route).expect("unsupported binding");
    assert!(matches!(
        binding.build(),
        Err(ProviderConstructionError::OpenRouterUnsupported)
    ));
}
