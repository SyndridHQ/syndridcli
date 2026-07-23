use super::super::credential_store::CredentialStore;
use super::super::credential_store::CredentialStoreError;
use super::super::provider_connection::AuthenticationMethod;
use super::super::provider_connection::ConnectionLabel;
use super::super::provider_connection::CredentialReference;
use super::super::provider_connection::ProviderConnection;
use super::super::provider_connection::ProviderConnectionId;
use super::super::provider_connection::ProviderId;
use super::AuthorizationCompletion;
use super::OpenRouterAuthConfiguration;
use super::OpenRouterAuthError;
use super::OpenRouterAuthorizationRequest;
use super::OpenRouterConnectionLifecycle;
use super::OpenRouterTokenExchangeRequest;
use super::OpenRouterTokenResponse;
use super::OpenRouterTokenTransport;
use super::PkceChallenge;
use super::PkceVerifier;
use crate::syndrid_orchestration::provider_connection::CredentialSecret;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;

const AUTH_CODE: &str = "authorization-code-sentinel";
const ACCESS_KEY: &str = "SYNDIRD_SECRET_MUST_NEVER_APPEAR_9f3d";
const VERIFIER_SENTINEL: &str = "PKCE_VERIFIER_MUST_NEVER_APPEAR";

#[derive(Clone, Default)]
struct MockStore {
    values: Arc<Mutex<Vec<(String, String)>>>,
    fail: bool,
}

impl CredentialStore for MockStore {
    fn store(
        &self,
        reference: &CredentialReference,
        secret: CredentialSecret,
    ) -> Result<(), CredentialStoreError> {
        if self.fail {
            return Err(CredentialStoreError::Unavailable);
        }
        self.values
            .lock()
            .expect("store lock")
            .push((reference.to_string(), secret.expose_for_auth().to_string()));
        Ok(())
    }

    fn retrieve(
        &self,
        reference: &CredentialReference,
    ) -> Result<CredentialSecret, CredentialStoreError> {
        let value = self
            .values
            .lock()
            .expect("store lock")
            .iter()
            .find(|(stored, _)| stored == reference.as_str())
            .map(|(_, value)| value.clone())
            .ok_or(CredentialStoreError::NotFound)?;
        CredentialSecret::new(value).map_err(|_| CredentialStoreError::Rejected)
    }

    fn delete(&self, reference: &CredentialReference) -> Result<(), CredentialStoreError> {
        let mut values = self.values.lock().expect("store lock");
        let before = values.len();
        values.retain(|(stored, _)| stored != reference.as_str());
        if values.len() == before {
            Err(CredentialStoreError::NotFound)
        } else {
            Ok(())
        }
    }

    fn contains(&self, reference: &CredentialReference) -> Result<bool, CredentialStoreError> {
        Ok(self
            .values
            .lock()
            .expect("store lock")
            .iter()
            .any(|(stored, _)| stored == reference.as_str()))
    }
}

#[derive(Clone)]
struct MockTransport {
    calls: Arc<Mutex<usize>>,
    result: Result<String, OpenRouterAuthError>,
}

impl OpenRouterTokenTransport for MockTransport {
    fn exchange(
        &self,
        request: OpenRouterTokenExchangeRequest,
    ) -> impl Future<Output = Result<OpenRouterTokenResponse, OpenRouterAuthError>> + Send {
        let calls = Arc::clone(&self.calls);
        let result = self.result.clone();
        async move {
            *calls.lock().expect("calls lock") += 1;
            let _ = request;
            result.map(|value| OpenRouterTokenResponse {
                access_key: CredentialSecret::new(value).expect("valid test access key"),
            })
        }
    }
}

fn configuration() -> OpenRouterAuthConfiguration {
    OpenRouterAuthConfiguration::default("http://127.0.0.1:43123/callback")
        .expect("default configuration")
}

fn completion(
    request: &OpenRouterAuthorizationRequest,
    state: &str,
    code: &str,
) -> AuthorizationCompletion {
    AuthorizationCompletion {
        state: state.to_string(),
        code: code.to_string(),
        callback_url: format!("{}&code={code}", request.redirect_uri()),
    }
}

fn connection() -> ProviderConnection {
    ProviderConnection::new(
        ProviderConnectionId::new("openrouter-connection").expect("connection ID"),
        ProviderId::new("openrouter").expect("provider ID"),
        ConnectionLabel::new("OpenRouter").expect("label"),
        AuthenticationMethod::OAuthPkce,
        Some(CredentialReference::new("openrouter-credential").expect("reference")),
        None,
        false,
    )
    .expect("connection")
}

fn lifecycle(
    store: MockStore,
    transport: MockTransport,
) -> OpenRouterConnectionLifecycle<MockStore, MockTransport> {
    OpenRouterConnectionLifecycle::new(configuration(), store, transport, connection())
        .expect("lifecycle")
}

#[test]
fn pkce_is_bounded_and_redacted() {
    let verifier = PkceVerifier::generate();
    assert!(!verifier.as_str().is_empty());
    assert!(verifier.as_str().len() <= 128);
    assert!(!format!("{verifier:?}").contains(VERIFIER_SENTINEL));
    assert!(!verifier.to_string().contains(VERIFIER_SENTINEL));
}

#[test]
fn pkce_s256_challenge_is_deterministic() {
    let verifier = PkceVerifier("fixed-verifier".to_string());
    assert_eq!(
        PkceChallenge::from_verifier(&verifier),
        PkceChallenge::from_verifier(&verifier)
    );
}

#[test]
fn authorization_request_contains_state_and_challenge_but_not_verifier() {
    let mut auth_lifecycle = lifecycle(
        MockStore::default(),
        MockTransport {
            calls: Arc::new(Mutex::new(0)),
            result: Ok(ACCESS_KEY.to_string()),
        },
    );
    let request = auth_lifecycle
        .begin_authorization()
        .expect("authorization session");
    assert!(request.authorization_url().contains(request.state.as_str()));
    assert!(
        request
            .authorization_url()
            .contains(request.code_challenge.as_str())
    );
    assert!(!request.authorization_url().contains("code_verifier"));
    assert!(!format!("{request:?}").contains(request.state.as_str()));
    let completion = completion(&request, request.state.as_str(), AUTH_CODE);
    assert!(!format!("{completion:?}").contains(AUTH_CODE));
}

#[test]
fn malformed_configuration_and_redirects_are_rejected() {
    assert!(matches!(
        OpenRouterAuthConfiguration::default("http://127.0.0.1:0/callback"),
        Err(OpenRouterAuthError::InvalidRedirectUri)
    ));
    assert!(OpenRouterAuthConfiguration::default("http://127.0.0.1:43123/callback").is_ok());
    assert!(matches!(
        OpenRouterAuthConfiguration::new(
            "not a URL",
            "https://openrouter.ai/api/v1/auth/keys",
            "http://127.0.0.1:1234/callback"
        ),
        Err(OpenRouterAuthError::InvalidAuthorizationEndpoint)
    ));
    assert!(matches!(
        OpenRouterAuthConfiguration::new(
            "https://openrouter.ai/auth",
            "https://openrouter.ai/api/v1/auth/keys",
            "http://user:pass@127.0.0.1:1234/callback"
        ),
        Err(OpenRouterAuthError::InvalidRedirectUri)
    ));
    assert!(matches!(
        AuthorizationCompletion::from_callback_url("http://127.0.0.1:1234/callback#code=secret"),
        Err(OpenRouterAuthError::InvalidCallback)
    ));
}

#[test]
fn authorization_states_are_distinct_and_redacted() {
    let mut lifecycle = lifecycle(
        MockStore::default(),
        MockTransport {
            calls: Arc::new(Mutex::new(0)),
            result: Ok(ACCESS_KEY.to_string()),
        },
    );
    let first = lifecycle.begin_authorization().expect("first request");
    let second = lifecycle.begin_authorization().expect("second request");
    assert_ne!(first.state, second.state);
    assert!(!format!("{:?}", first.state).contains(first.state.as_str()));
    assert!(!first.state.to_string().contains(first.state.as_str()));
}

#[test]
fn token_exchange_body_contains_no_bearer_authorization_field() {
    let body = serde_json::to_value(super::OpenRouterTokenRequest {
        code: AUTH_CODE,
        code_verifier: "verifier",
        code_challenge_method: "S256",
    })
    .expect("token request body");
    assert!(body.get("authorization").is_none());
    assert!(body.get("Authorization").is_none());
}

#[tokio::test]
async fn callback_target_must_match_pending_port_and_path() {
    let mut callback_lifecycle = lifecycle(
        MockStore::default(),
        MockTransport {
            calls: Arc::new(Mutex::new(0)),
            result: Ok(ACCESS_KEY.to_string()),
        },
    );
    let request = callback_lifecycle.begin_authorization().expect("request");
    let wrong_port = format!(
        "http://127.0.0.1:43124/callback?state={}&code={AUTH_CODE}",
        request.state.as_str()
    );
    assert_eq!(
        callback_lifecycle
            .complete_authorization(
                AuthorizationCompletion::from_callback_url(wrong_port).expect("callback")
            )
            .await,
        Err(OpenRouterAuthError::InvalidCallback)
    );

    let mut path_lifecycle = lifecycle(
        MockStore::default(),
        MockTransport {
            calls: Arc::new(Mutex::new(0)),
            result: Ok(ACCESS_KEY.to_string()),
        },
    );
    let request = path_lifecycle.begin_authorization().expect("request");
    let wrong_path = format!(
        "http://127.0.0.1:43123/other?state={}&code={AUTH_CODE}",
        request.state.as_str()
    );
    assert_eq!(
        path_lifecycle
            .complete_authorization(
                AuthorizationCompletion::from_callback_url(wrong_path).expect("callback")
            )
            .await,
        Err(OpenRouterAuthError::InvalidCallback)
    );
}

#[tokio::test]
async fn state_mismatch_and_missing_code_do_not_exchange() {
    let calls = Arc::new(Mutex::new(0));
    let existing_store = MockStore::default();
    existing_store
        .store(
            &CredentialReference::new("openrouter-credential").unwrap(),
            CredentialSecret::new("existing-credential").unwrap(),
        )
        .expect("existing credential");
    let mut first_lifecycle = lifecycle(
        existing_store.clone(),
        MockTransport {
            calls: Arc::clone(&calls),
            result: Ok(ACCESS_KEY.to_string()),
        },
    );
    let first_request = first_lifecycle.begin_authorization().expect("request");
    assert_eq!(
        first_lifecycle
            .complete_authorization(completion(&first_request, "wrong-state", AUTH_CODE))
            .await,
        Err(OpenRouterAuthError::StateMismatch)
    );
    assert_eq!(*calls.lock().expect("calls lock"), 0);
    assert_eq!(
        existing_store
            .retrieve(&CredentialReference::new("openrouter-credential").unwrap())
            .unwrap()
            .expose_for_auth(),
        "existing-credential"
    );

    let mut second_lifecycle = lifecycle(
        MockStore::default(),
        MockTransport {
            calls: Arc::clone(&calls),
            result: Ok(ACCESS_KEY.to_string()),
        },
    );
    let second_request = second_lifecycle.begin_authorization().expect("request");
    assert_eq!(
        second_lifecycle
            .complete_authorization(completion(
                &second_request,
                second_request.state.as_str(),
                " ",
            ))
            .await,
        Err(OpenRouterAuthError::MissingAuthorizationCode)
    );
    assert_eq!(*calls.lock().expect("calls lock"), 0);
}

#[tokio::test]
async fn successful_exchange_stores_only_opaque_metadata() {
    let calls = Arc::new(Mutex::new(0));
    let store = MockStore::default();
    let mut lifecycle = lifecycle(
        store.clone(),
        MockTransport {
            calls: Arc::clone(&calls),
            result: Ok(ACCESS_KEY.to_string()),
        },
    );
    let request = lifecycle.begin_authorization().expect("request");
    let result = lifecycle
        .complete_authorization(completion(&request, request.state.as_str(), AUTH_CODE))
        .await
        .expect("exchange");
    assert_eq!(*calls.lock().expect("calls lock"), 1);
    assert_eq!(
        result
            .credential_reference
            .as_ref()
            .expect("credential reference")
            .as_str(),
        "openrouter-credential"
    );
    assert!(!format!("{result:?}").contains(ACCESS_KEY));
    assert_eq!(
        store
            .retrieve(&CredentialReference::new("openrouter-credential").unwrap())
            .unwrap()
            .expose_for_auth(),
        ACCESS_KEY
    );
}

#[tokio::test]
async fn transport_and_store_failures_do_not_authenticate() {
    let calls = Arc::new(Mutex::new(0));
    let mut transport_failure = lifecycle(
        MockStore::default(),
        MockTransport {
            calls: Arc::clone(&calls),
            result: Err(OpenRouterAuthError::Unauthorized),
        },
    );
    let request = transport_failure.begin_authorization().expect("request");
    assert_eq!(
        transport_failure
            .complete_authorization(completion(&request, request.state.as_str(), AUTH_CODE))
            .await,
        Err(OpenRouterAuthError::Unauthorized)
    );
    assert!(!transport_failure.connection().enabled);

    let existing_store = MockStore::default();
    existing_store
        .store(
            &CredentialReference::new("openrouter-credential").unwrap(),
            CredentialSecret::new("existing-credential").unwrap(),
        )
        .expect("existing credential");
    let mut store_failure = lifecycle(
        MockStore {
            values: existing_store.values.clone(),
            fail: true,
        },
        MockTransport {
            calls,
            result: Ok(ACCESS_KEY.to_string()),
        },
    );
    let request = store_failure.begin_authorization().expect("request");
    assert_eq!(
        store_failure
            .complete_authorization(completion(&request, request.state.as_str(), AUTH_CODE))
            .await,
        Err(OpenRouterAuthError::CredentialStoreUnavailable)
    );
    assert!(!store_failure.connection().enabled);
    assert_eq!(
        existing_store
            .retrieve(&CredentialReference::new("openrouter-credential").unwrap())
            .unwrap()
            .expose_for_auth(),
        "existing-credential"
    );
}

#[tokio::test]
async fn sessions_are_single_use_and_disconnect_is_local() {
    let store = MockStore::default();
    let calls = Arc::new(Mutex::new(0));
    let mut lifecycle = lifecycle(
        store.clone(),
        MockTransport {
            calls,
            result: Ok(ACCESS_KEY.to_string()),
        },
    );
    let request = lifecycle.begin_authorization().expect("request");
    let completion = AuthorizationCompletion::from_callback_url(format!(
        "{}&code={AUTH_CODE}",
        request.redirect_uri()
    ))
    .expect("callback");
    lifecycle
        .complete_authorization(completion.clone())
        .await
        .expect("exchange");
    assert_eq!(
        lifecycle.complete_authorization(completion).await,
        Err(OpenRouterAuthError::SessionAlreadyUsed)
    );
    lifecycle.disconnect_local().expect("local disconnect");
    assert!(
        !store
            .contains(&CredentialReference::new("openrouter-credential").unwrap())
            .expect("contains")
    );
}

#[test]
fn error_output_is_static_and_secret_safe() {
    for error in [
        OpenRouterAuthError::Unauthorized,
        OpenRouterAuthError::CredentialStoreRejected,
        OpenRouterAuthError::InvalidTokenResponse,
    ] {
        assert!(!format!("{error:?}").contains(ACCESS_KEY));
        assert!(!error.to_string().contains(ACCESS_KEY));
        assert!(!error.to_string().contains(AUTH_CODE));
    }
}
