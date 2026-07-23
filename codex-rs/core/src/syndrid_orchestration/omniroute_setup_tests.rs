use super::*;
use crate::syndrid_orchestration::credential_store::CredentialStore;
use crate::syndrid_orchestration::credential_store::CredentialStoreError;
use crate::syndrid_orchestration::provider_connection::CredentialReference;
use crate::syndrid_orchestration::provider_connection::CredentialSecret;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tempfile::tempdir;
use tokio_util::sync::CancellationToken;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

struct Store {
    stores: AtomicUsize,
}

impl CredentialStore for Store {
    fn store(
        &self,
        _reference: &CredentialReference,
        _secret: CredentialSecret,
    ) -> Result<(), CredentialStoreError> {
        self.stores.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn retrieve(&self, _: &CredentialReference) -> Result<CredentialSecret, CredentialStoreError> {
        Err(CredentialStoreError::NotFound)
    }

    fn delete(&self, _: &CredentialReference) -> Result<(), CredentialStoreError> {
        Ok(())
    }

    fn contains(&self, _: &CredentialReference) -> Result<bool, CredentialStoreError> {
        Ok(false)
    }
}

impl CredentialStore for &Store {
    fn store(
        &self,
        reference: &CredentialReference,
        secret: CredentialSecret,
    ) -> Result<(), CredentialStoreError> {
        (*self).store(reference, secret)
    }

    fn retrieve(
        &self,
        reference: &CredentialReference,
    ) -> Result<CredentialSecret, CredentialStoreError> {
        (*self).retrieve(reference)
    }

    fn delete(&self, reference: &CredentialReference) -> Result<(), CredentialStoreError> {
        (*self).delete(reference)
    }

    fn contains(&self, reference: &CredentialReference) -> Result<bool, CredentialStoreError> {
        (*self).contains(reference)
    }
}

fn request(base_url: String) -> OmniRouteConnectionSetupRequest {
    OmniRouteConnectionSetupRequest {
        connection_id: "omniroute-local".to_string(),
        label: "Local OmniRoute".to_string(),
        base_url,
        credential_reference: "omniroute-local-key".to_string(),
        api_key: "omniroute-api-key-sentinel".to_string(),
        allow_remote_https: false,
    }
}

#[test]
fn local_base_url_policy_accepts_default_and_loopback_forms() {
    for value in [
        OMNIROUTE_DEFAULT_BASE_URL,
        "http://127.0.0.1:3000",
        "https://localhost:3000",
        "https://127.0.0.1:3000",
    ] {
        assert!(validate_base_url(value, false).is_ok(), "{value}");
    }
}

#[test]
fn remote_http_and_unsafe_urls_are_rejected() {
    for value in [
        "http://example.com:20128",
        "http://localhost:0",
        "http://user:pass@localhost:20128",
        "http://localhost:20128/?token=secret",
        "http://localhost:20128/#fragment",
        "http://0.0.0.0:20128",
    ] {
        assert!(validate_base_url(value, false).is_err(), "{value}");
    }
    assert!(validate_base_url("https://example.com:20128", false).is_err());
    assert!(validate_base_url("https://example.com:20128", true).is_ok());
}

#[tokio::test]
async fn successful_validation_stores_once_and_never_persists_secret() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Bearer omniroute-api-key-sentinel"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(serde_json::json!({
                    "object": "list",
                    "data": [{"id": "omniroute/model-a", "object": "model"}]
                })),
        )
        .mount(&server)
        .await;
    let store = Store {
        stores: AtomicUsize::new(0),
    };
    let metadata =
        setup_omniroute_with_dependencies(request(server.uri()), &store, CancellationToken::new())
            .await
            .expect("setup");
    assert_eq!(store.stores.load(Ordering::SeqCst), 1);
    assert_eq!(metadata.models, vec!["omniroute/model-a"]);
    let serialized = serde_json::to_string(&metadata).expect("metadata JSON");
    assert!(!serialized.contains("omniroute-api-key-sentinel"));
    assert!(!format!("{metadata:?}").contains("omniroute-api-key-sentinel"));
    assert!(!metadata.to_string().contains("omniroute-api-key-sentinel"));
}

#[tokio::test]
async fn failed_validation_and_cancellation_do_not_store() {
    let store = Store {
        stores: AtomicUsize::new(0),
    };
    let error = setup_omniroute_with_dependencies(
        request("http://example.com:20128".to_string()),
        &store,
        CancellationToken::new(),
    )
    .await
    .expect_err("remote HTTP");
    assert_eq!(error, OmniRouteSetupError::RemotePlaintextRejected);
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = setup_omniroute_with_dependencies(
        request(OMNIROUTE_DEFAULT_BASE_URL.to_string()),
        &store,
        cancellation,
    )
    .await
    .expect_err("cancelled setup");
    assert_eq!(error, OmniRouteSetupError::Cancelled);
    assert_eq!(store.stores.load(Ordering::SeqCst), 0);
}

#[test]
fn registry_supports_named_connections_and_safe_atomic_persistence() {
    let directory = tempdir().expect("tempdir");
    let path = directory.path().join("providers.json");
    let metadata = OmniRouteConnectionMetadata {
        connection_id: "omniroute-local".to_string(),
        provider_id: OMNIROUTE_PROVIDER_ID.to_string(),
        label: "Local OmniRoute".to_string(),
        base_url: OMNIROUTE_DEFAULT_BASE_URL.to_string(),
        credential_reference: "omniroute-local-key".to_string(),
        enabled: true,
        validation: ConnectionValidationResult::valid(),
        models: vec!["omniroute/model-a".to_string()],
        validated_at: Some(1),
    };
    let mut registry = OmniRouteRegistry::default();
    registry.insert(metadata.clone()).expect("insert");
    assert_eq!(
        registry.insert(metadata.clone()),
        Err(OmniRouteRegistryError::DuplicateConnection)
    );
    registry.save(&path).expect("save");
    let loaded = OmniRouteRegistry::load(&path).expect("load");
    assert_eq!(loaded.get("omniroute-local"), Some(&metadata));
    assert!(
        !std::fs::read_to_string(path)
            .expect("registry")
            .contains("omniroute-api-key-sentinel")
    );
}
