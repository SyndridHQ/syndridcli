use super::*;
use crate::syndrid_orchestration::openai_compatible::OpenAiCompatibleResponse;
use crate::syndrid_orchestration::provider_connection::ConnectionLabel;
use crate::syndrid_orchestration::provider_connection::ConnectionValidationResult;
use crate::syndrid_orchestration::provider_connection::CredentialReference;
use crate::syndrid_orchestration::provider_connection::CredentialSecret;
use crate::syndrid_orchestration::provider_connection::ProviderConnectionId;
use crate::syndrid_orchestration::provider_connection::ProviderId;
use pretty_assertions::assert_eq;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

struct TestStore {
    retrieves: AtomicUsize,
    error: Option<CredentialStoreError>,
    secret: String,
}

impl CredentialStore for TestStore {
    fn store(
        &self,
        _reference: &CredentialReference,
        _secret: CredentialSecret,
    ) -> Result<(), CredentialStoreError> {
        unimplemented!()
    }

    fn retrieve(
        &self,
        _reference: &CredentialReference,
    ) -> Result<CredentialSecret, CredentialStoreError> {
        self.retrieves.fetch_add(1, Ordering::SeqCst);
        if let Some(error) = self.error {
            return Err(error);
        }
        CredentialSecret::new(self.secret.clone()).map_err(|_| CredentialStoreError::Rejected)
    }

    fn delete(&self, _reference: &CredentialReference) -> Result<(), CredentialStoreError> {
        unimplemented!()
    }

    fn contains(&self, _reference: &CredentialReference) -> Result<bool, CredentialStoreError> {
        unimplemented!()
    }
}

struct TestTransport {
    error: Option<OpenAiCompatibleTransportError>,
}

impl OpenAiCompatibleTransport for TestTransport {
    async fn invoke(
        &self,
        bearer: &str,
        _request: OpenAiCompatibleRequest,
        _cancellation: CancellationToken,
    ) -> Result<OpenAiCompatibleResponse, OpenAiCompatibleTransportError> {
        assert_eq!(bearer, "credential-sentinel");
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        Ok(OpenAiCompatibleResponse {
            text: "generated text".to_string(),
            model: Some("openrouter/test-model".to_string()),
            finish_reason: Some("stop".to_string()),
            usage: None,
            request_id: Some("request-id".to_string()),
        })
    }
}

fn connection() -> ProviderConnection {
    let mut connection = ProviderConnection::new(
        ProviderConnectionId::new("openrouter-default").expect("connection ID"),
        ProviderId::new(OPENROUTER_PROVIDER_ID).expect("provider ID"),
        ConnectionLabel::new("OpenRouter").expect("label"),
        AuthenticationMethod::OAuthPkce,
        Some(CredentialReference::new("openrouter-credential").expect("reference")),
        None,
        true,
    )
    .expect("connection");
    connection.validation = ConnectionValidationResult::valid();
    connection
}

#[tokio::test]
async fn adapter_retrieves_credential_once_and_maps_provider_output() {
    let store = TestStore {
        retrieves: AtomicUsize::new(0),
        error: None,
        secret: "credential-sentinel".to_string(),
    };
    let adapter = OpenRouterInvocationAdapter::new(
        connection(),
        store,
        TestTransport { error: None },
        OpenRouterInvocationConfiguration::new("openrouter/test-model").expect("configuration"),
    );
    let result = adapter
        .invoke(
            ProviderInvocationRequest {
                provider: OPENROUTER_PROVIDER_ID.to_string(),
                model: "openrouter/test-model".to_string(),
                system: None,
                user: "user prompt".to_string(),
                max_output_tokens: 128,
            },
            CancellationToken::new(),
        )
        .await
        .expect("invocation");

    assert_eq!(result.provider, OPENROUTER_PROVIDER_ID);
    assert_eq!(result.text, "generated text");
    assert_eq!(result.request_id.as_deref(), Some("request-id"));
    assert_eq!(adapter.store.retrieves.load(Ordering::SeqCst), 1);
    let debug = format!("{adapter:?}");
    assert!(!debug.contains("credential-sentinel"));
}

#[tokio::test]
async fn invalid_connection_is_rejected_before_credential_retrieval() {
    let mut connection = connection();
    connection.enabled = false;
    let store = TestStore {
        retrieves: AtomicUsize::new(0),
        error: None,
        secret: "credential-sentinel".to_string(),
    };
    let adapter = OpenRouterInvocationAdapter::new(
        connection,
        store,
        TestTransport { error: None },
        OpenRouterInvocationConfiguration::new("openrouter/test-model").expect("configuration"),
    );
    let result = adapter
        .invoke(
            ProviderInvocationRequest {
                provider: OPENROUTER_PROVIDER_ID.to_string(),
                model: "openrouter/test-model".to_string(),
                system: None,
                user: "user prompt".to_string(),
                max_output_tokens: 128,
            },
            CancellationToken::new(),
        )
        .await;
    assert_eq!(result, Err(ProviderInvocationError::ConnectionDisabled));
    assert_eq!(adapter.store.retrieves.load(Ordering::SeqCst), 0);
}

fn invocation_request() -> ProviderInvocationRequest {
    ProviderInvocationRequest {
        provider: OPENROUTER_PROVIDER_ID.to_string(),
        model: "openrouter/test-model".to_string(),
        system: Some("system-sentinel".to_string()),
        user: "user-sentinel".to_string(),
        max_output_tokens: 128,
    }
}

#[tokio::test]
async fn connection_validation_rejects_before_credential_retrieval() {
    let mut cases = Vec::new();
    let mut wrong_provider = connection();
    wrong_provider.provider_id = ProviderId::new("other-provider").expect("provider");
    cases.push((wrong_provider, ProviderInvocationError::UnsupportedProvider));
    let mut unsupported_authentication = connection();
    unsupported_authentication.authentication_method = AuthenticationMethod::ApiKey;
    cases.push((
        unsupported_authentication,
        ProviderInvocationError::UnsupportedAuthenticationMethod,
    ));
    let mut disabled = connection();
    disabled.enabled = false;
    cases.push((disabled, ProviderInvocationError::ConnectionDisabled));
    let mut unvalidated = connection();
    unvalidated.validation = ConnectionValidationResult::unvalidated();
    cases.push((unvalidated, ProviderInvocationError::ConnectionUnvalidated));
    let mut missing_reference = connection();
    missing_reference.credential_reference = None;
    cases.push((
        missing_reference,
        ProviderInvocationError::MissingCredentialReference,
    ));
    for (connection, expected) in cases {
        let store = TestStore {
            retrieves: AtomicUsize::new(0),
            error: None,
            secret: "credential-sentinel".to_string(),
        };
        let adapter = OpenRouterInvocationAdapter::new(
            connection,
            store,
            TestTransport { error: None },
            OpenRouterInvocationConfiguration::new("openrouter/test-model").expect("configuration"),
        );
        assert_eq!(
            adapter
                .invoke(invocation_request(), CancellationToken::new())
                .await,
            Err(expected)
        );
        assert_eq!(adapter.store.retrieves.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn credential_store_failures_map_without_exposing_secrets() {
    for (store_error, expected) in [
        (
            CredentialStoreError::NotFound,
            ProviderInvocationError::CredentialNotFound,
        ),
        (
            CredentialStoreError::Unavailable,
            ProviderInvocationError::CredentialStoreUnavailable,
        ),
        (
            CredentialStoreError::Rejected,
            ProviderInvocationError::CredentialStoreRejected,
        ),
    ] {
        let store = TestStore {
            retrieves: AtomicUsize::new(0),
            error: Some(store_error),
            secret: "credential-sentinel".to_string(),
        };
        let adapter = OpenRouterInvocationAdapter::new(
            connection(),
            store,
            TestTransport { error: None },
            OpenRouterInvocationConfiguration::new("openrouter/test-model").expect("configuration"),
        );
        let error = adapter
            .invoke(invocation_request(), CancellationToken::new())
            .await
            .expect_err("store error");
        assert_eq!(error, expected);
        assert!(!error.to_string().contains("credential-sentinel"));
    }
}

#[test]
fn invocation_values_debug_are_redacted() {
    let request = ProviderInvocationRequest {
        provider: OPENROUTER_PROVIDER_ID.to_string(),
        model: "model-sentinel".to_string(),
        system: Some("system-sentinel".to_string()),
        user: "user-sentinel".to_string(),
        max_output_tokens: 128,
    };
    let result = ProviderInvocationResult {
        provider: OPENROUTER_PROVIDER_ID.to_string(),
        model: "model-sentinel".to_string(),
        text: "output-sentinel".to_string(),
        finish_reason: None,
        usage: None,
        request_id: None,
    };
    let debug = format!("{request:?} {result:?}");
    for sentinel in [
        "bearer-sentinel",
        "system-sentinel",
        "user-sentinel",
        "output-sentinel",
    ] {
        assert!(!debug.contains(sentinel), "debug leaked {sentinel}");
    }
}
