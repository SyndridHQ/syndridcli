use super::*;
use crate::CodexAccountConnectionMetadata;
use crate::CodexAccountProfileId;
use crate::ConnectionValidationStatus;
use crate::syndrid_orchestration::invocation::ProviderInvocation;
use crate::syndrid_orchestration::invocation::ProviderInvocationError;
use std::sync::Arc;
use std::sync::Mutex;

fn envelope() -> CodexCredentialEnvelope {
    CodexCredentialEnvelope::parse(
        &serde_json::json!({
            "schema_version": 1,
            "credential_kind": "chatgpt_oauth",
            "payload": {
                "id_token": "eyJhbGciOiJub25lIn0.eyJzdWIiOiIxIn0.sig",
                "access_token": "access",
                "refresh_token": "refresh",
                "account_id": "account"
            }
        })
        .to_string(),
    )
    .expect("envelope")
}

#[tokio::test]
async fn unavailable_client_is_explicit_and_bounded() {
    let error = UnavailableCodexInvocationClient
        .invoke(
            &envelope(),
            ProviderInvocationRequest {
                provider: "codex".to_string(),
                model: "model".to_string(),
                system: None,
                user: "prompt-sentinel".to_string(),
                max_output_tokens: 32,
            },
            CancellationToken::new(),
        )
        .await
        .expect_err("live invocation must be explicitly unavailable");
    assert_eq!(
        error,
        ProviderInvocationError::LiveCodexInvocationUnavailable
    );
    assert!(!error.to_string().contains("prompt-sentinel"));
}

#[derive(Clone)]
struct TestCredentialProvider {
    envelope: CodexCredentialEnvelope,
    requested: Arc<Mutex<Vec<String>>>,
    failure: Option<ProviderInvocationError>,
}

impl CodexCredentialProvider for TestCredentialProvider {
    fn retrieve(
        &self,
        connection_id: &str,
    ) -> Result<CodexCredentialEnvelope, ProviderInvocationError> {
        self.requested
            .lock()
            .expect("requested lock")
            .push(connection_id.to_string());
        if let Some(error) = self.failure {
            return Err(error);
        }
        Ok(self.envelope.clone())
    }
}

#[derive(Clone)]
struct TestInvocationClient {
    requests: Arc<Mutex<Vec<ProviderInvocationRequest>>>,
    failure: Option<ProviderInvocationError>,
}

impl CodexInvocationClient for TestInvocationClient {
    async fn invoke(
        &self,
        _credential: &CodexCredentialEnvelope,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        if let Some(error) = self.failure {
            return Err(error);
        }
        if cancellation.is_cancelled() {
            return Err(ProviderInvocationError::Cancelled);
        }
        self.requests
            .lock()
            .expect("request lock")
            .push(request.clone());
        Ok(ProviderInvocationResult {
            provider: request.provider,
            model: request.model,
            text: "generated-output-sentinel".to_string(),
            finish_reason: Some("stop".to_string()),
            usage: None,
            request_id: None,
        })
    }
}

fn account(connection_id: &str) -> CodexAccountConnectionMetadata {
    CodexAccountConnectionMetadata {
        connection_id: connection_id.to_string(),
        profile_id: CodexAccountProfileId::new(connection_id).expect("profile id"),
        provider_id: "codex".to_string(),
        label: connection_id.to_string(),
        state: CodexAccountProfileState::Connected,
        account_email: None,
        account_id: None,
        plan_label: None,
        enabled: true,
        validation: ConnectionValidationStatus::Valid,
        last_authenticated_at: None,
        last_validated_at: None,
        credential_reference: CodexAccountProfileRegistry::credential_reference_for(connection_id)
            .expect("ref"),
        schema_version: 1,
    }
}

#[tokio::test]
async fn exact_selected_connection_retrieves_once_and_maps_result() {
    let mut accounts = CodexAccountProfileRegistry::default();
    accounts
        .insert(account("codex-personal"))
        .expect("personal");
    accounts
        .insert(account("codex-secondary"))
        .expect("secondary");
    let requested = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let adapter = CodexInvocationAdapter::with_credential_provider(
        ProviderSelection::new("codex-personal", "codex", "model-id").expect("selection"),
        accounts,
        TestCredentialProvider {
            envelope: envelope(),
            requested: Arc::clone(&requested),
            failure: None,
        },
        TestInvocationClient {
            requests: Arc::clone(&requests),
            failure: None,
        },
    );
    let result = adapter
        .invoke(
            ProviderInvocationRequest {
                provider: "codex".to_string(),
                model: "model-id".to_string(),
                system: None,
                user: "prompt-sentinel".to_string(),
                max_output_tokens: 32,
            },
            CancellationToken::new(),
        )
        .await
        .expect("invocation");
    assert_eq!(
        requested.lock().expect("requested lock").as_slice(),
        ["codex-personal"]
    );
    assert_eq!(requests.lock().expect("request lock").len(), 1);
    assert_eq!(result.provider, "codex");
    assert_eq!(result.model, "model-id");
    assert_eq!(result.text, "generated-output-sentinel");
    let debug = format!("{result:?}");
    assert!(!debug.contains("prompt-sentinel"));
    assert!(!debug.contains("generated-output-sentinel"));
}

#[tokio::test]
async fn secondary_selection_isolated_and_account_failures_do_not_fallback() {
    let mut accounts = CodexAccountProfileRegistry::default();
    accounts
        .insert(account("codex-personal"))
        .expect("personal");
    accounts
        .insert(account("codex-secondary"))
        .expect("secondary");
    let requested = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let adapter = CodexInvocationAdapter::with_credential_provider(
        ProviderSelection::new("codex-secondary", "codex", "model-id").expect("selection"),
        accounts,
        TestCredentialProvider {
            envelope: envelope(),
            requested: Arc::clone(&requested),
            failure: None,
        },
        TestInvocationClient {
            requests,
            failure: Some(ProviderInvocationError::RateLimited),
        },
    );
    assert_eq!(
        adapter
            .invoke(
                ProviderInvocationRequest {
                    provider: "codex".to_string(),
                    model: "model-id".to_string(),
                    system: None,
                    user: "prompt".to_string(),
                    max_output_tokens: 32,
                },
                CancellationToken::new(),
            )
            .await,
        Err(ProviderInvocationError::RateLimited)
    );
    assert_eq!(
        requested.lock().expect("requested lock").as_slice(),
        ["codex-secondary"]
    );
}

#[tokio::test]
async fn invalid_envelope_fails_before_client_and_state_blocks_retrieval() {
    let mut accounts = CodexAccountProfileRegistry::default();
    let mut disabled = account("codex-disabled");
    disabled.enabled = false;
    accounts.insert(disabled).expect("disabled");
    let requested = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let disabled_adapter = CodexInvocationAdapter::with_credential_provider(
        ProviderSelection::new("codex-disabled", "codex", "model").expect("selection"),
        accounts.clone(),
        TestCredentialProvider {
            envelope: envelope(),
            requested: Arc::clone(&requested),
            failure: Some(ProviderInvocationError::InvalidResponse),
        },
        TestInvocationClient {
            requests: Arc::clone(&requests),
            failure: None,
        },
    );
    assert_eq!(
        disabled_adapter
            .invoke(
                ProviderInvocationRequest {
                    provider: "codex".to_string(),
                    model: "model".to_string(),
                    system: None,
                    user: "prompt".to_string(),
                    max_output_tokens: 32,
                },
                CancellationToken::new(),
            )
            .await,
        Err(ProviderInvocationError::ConnectionDisabled)
    );
    assert!(requested.lock().expect("requested lock").is_empty());

    let mut reauth = account("codex-reauth");
    reauth.state = CodexAccountProfileState::ReauthenticationRequired;
    let mut reauth_accounts = CodexAccountProfileRegistry::default();
    reauth_accounts.insert(reauth).expect("reauth account");
    let reauth_requested = Arc::new(Mutex::new(Vec::new()));
    let reauth_adapter = CodexInvocationAdapter::with_credential_provider(
        ProviderSelection::new("codex-reauth", "codex", "model").expect("selection"),
        reauth_accounts,
        TestCredentialProvider {
            envelope: envelope(),
            requested: Arc::clone(&reauth_requested),
            failure: None,
        },
        TestInvocationClient {
            requests: Arc::new(Mutex::new(Vec::new())),
            failure: None,
        },
    );
    assert_eq!(
        reauth_adapter
            .invoke(
                ProviderInvocationRequest {
                    provider: "codex".to_string(),
                    model: "model".to_string(),
                    system: None,
                    user: "prompt".to_string(),
                    max_output_tokens: 32,
                },
                CancellationToken::new(),
            )
            .await,
        Err(ProviderInvocationError::ConnectionUnvalidated)
    );
    assert!(reauth_requested.lock().expect("requested lock").is_empty());

    let mut connected = account("codex-invalid");
    connected.profile_id = CodexAccountProfileId::new("codex-invalid").expect("profile id");
    let mut invalid_accounts = CodexAccountProfileRegistry::default();
    invalid_accounts.insert(connected).expect("invalid account");
    let invalid_requested = Arc::new(Mutex::new(Vec::new()));
    let invalid_adapter = CodexInvocationAdapter::with_credential_provider(
        ProviderSelection::new("codex-invalid", "codex", "model").expect("selection"),
        invalid_accounts,
        TestCredentialProvider {
            envelope: envelope(),
            requested: Arc::clone(&invalid_requested),
            failure: Some(ProviderInvocationError::InvalidResponse),
        },
        TestInvocationClient {
            requests,
            failure: None,
        },
    );
    assert_eq!(
        invalid_adapter
            .invoke(
                ProviderInvocationRequest {
                    provider: "codex".to_string(),
                    model: "model".to_string(),
                    system: None,
                    user: "prompt".to_string(),
                    max_output_tokens: 32,
                },
                CancellationToken::new(),
            )
            .await,
        Err(ProviderInvocationError::InvalidResponse)
    );
    assert_eq!(
        invalid_requested.lock().expect("requested lock").as_slice(),
        ["codex-invalid"]
    );
}

#[tokio::test]
async fn cancellation_and_timeout_errors_are_not_retried() {
    let mut accounts = CodexAccountProfileRegistry::default();
    accounts
        .insert(account("codex-personal"))
        .expect("personal");
    let requested = Arc::new(Mutex::new(Vec::new()));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let adapter = CodexInvocationAdapter::with_credential_provider(
        ProviderSelection::new("codex-personal", "codex", "model").expect("selection"),
        accounts,
        TestCredentialProvider {
            envelope: envelope(),
            requested: Arc::clone(&requested),
            failure: None,
        },
        TestInvocationClient {
            requests,
            failure: None,
        },
    );
    assert_eq!(
        adapter
            .invoke(
                ProviderInvocationRequest {
                    provider: "codex".to_string(),
                    model: "model".to_string(),
                    system: None,
                    user: "prompt".to_string(),
                    max_output_tokens: 32,
                },
                cancellation,
            )
            .await,
        Err(ProviderInvocationError::Cancelled)
    );
    assert_eq!(requested.lock().expect("requested lock").len(), 1);

    let mut timeout_accounts = CodexAccountProfileRegistry::default();
    timeout_accounts
        .insert(account("codex-personal"))
        .expect("personal");
    let timeout_adapter = CodexInvocationAdapter::with_credential_provider(
        ProviderSelection::new("codex-personal", "codex", "model").expect("selection"),
        timeout_accounts,
        TestCredentialProvider {
            envelope: envelope(),
            requested: Arc::new(Mutex::new(Vec::new())),
            failure: None,
        },
        TestInvocationClient {
            requests: Arc::new(Mutex::new(Vec::new())),
            failure: Some(ProviderInvocationError::RequestTimedOut),
        },
    );
    assert_eq!(
        timeout_adapter
            .invoke(
                ProviderInvocationRequest {
                    provider: "codex".to_string(),
                    model: "model".to_string(),
                    system: None,
                    user: "prompt".to_string(),
                    max_output_tokens: 32,
                },
                CancellationToken::new(),
            )
            .await,
        Err(ProviderInvocationError::RequestTimedOut)
    );
}
