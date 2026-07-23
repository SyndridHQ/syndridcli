use super::*;
use crate::syndrid_orchestration::credential_store::CredentialStore;
use crate::syndrid_orchestration::credential_store::CredentialStoreError;
use crate::syndrid_orchestration::openai_compatible::OpenAiCompatibleRequest;
use crate::syndrid_orchestration::openai_compatible::OpenAiCompatibleResponse;
use crate::syndrid_orchestration::openai_compatible::OpenAiCompatibleTransport;
use crate::syndrid_orchestration::openai_compatible::OpenAiCompatibleTransportError;
use crate::syndrid_orchestration::provider_connection::CredentialReference;
use crate::syndrid_orchestration::provider_connection::CredentialSecret;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use tokio_util::sync::CancellationToken;

struct Store {
    retrieves: AtomicUsize,
}

impl CredentialStore for Store {
    fn store(
        &self,
        _: &CredentialReference,
        _: CredentialSecret,
    ) -> Result<(), CredentialStoreError> {
        Ok(())
    }
    fn retrieve(&self, _: &CredentialReference) -> Result<CredentialSecret, CredentialStoreError> {
        self.retrieves.fetch_add(1, Ordering::SeqCst);
        CredentialSecret::new("omniroute-api-key-sentinel")
            .map_err(|_| CredentialStoreError::Rejected)
    }
    fn delete(&self, _: &CredentialReference) -> Result<(), CredentialStoreError> {
        Ok(())
    }
    fn contains(&self, _: &CredentialReference) -> Result<bool, CredentialStoreError> {
        Ok(true)
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

struct Transport;

impl OpenAiCompatibleTransport for Transport {
    async fn invoke(
        &self,
        bearer: &str,
        request: OpenAiCompatibleRequest,
        _: CancellationToken,
    ) -> Result<OpenAiCompatibleResponse, OpenAiCompatibleTransportError> {
        assert_eq!(bearer, "omniroute-api-key-sentinel");
        assert_eq!(request.model, "omniroute/model-a");
        Ok(OpenAiCompatibleResponse {
            text: "omniroute-output-sentinel".to_string(),
            model: Some(request.model),
            finish_reason: Some("stop".to_string()),
            usage: None,
            request_id: None,
        })
    }
}

fn connection() -> OmniRouteConnectionMetadata {
    OmniRouteConnectionMetadata {
        connection_id: "omniroute-local".to_string(),
        provider_id: OMNIROUTE_PROVIDER_ID.to_string(),
        label: "Local OmniRoute".to_string(),
        base_url: OMNIROUTE_DEFAULT_BASE_URL.to_string(),
        credential_reference: "omniroute-local-key".to_string(),
        enabled: true,
        validation: ConnectionValidationResult::valid(),
        models: vec!["omniroute/model-a".to_string()],
        validated_at: Some(1),
    }
}

fn request() -> ProviderInvocationRequest {
    ProviderInvocationRequest {
        provider: OMNIROUTE_PROVIDER_ID.to_string(),
        model: "omniroute/model-a".to_string(),
        system: Some("omniroute-system-sentinel".to_string()),
        user: "omniroute-prompt-sentinel".to_string(),
        max_output_tokens: 128,
    }
}

#[tokio::test]
async fn adapter_retrieves_once_and_maps_provider_neutral_result() {
    let store = Store {
        retrieves: AtomicUsize::new(0),
    };
    let adapter = OmniRouteInvocationAdapter::new(connection(), &store, Transport);
    let result = adapter
        .invoke(request(), CancellationToken::new())
        .await
        .expect("invoke");
    assert_eq!(result.provider, OMNIROUTE_PROVIDER_ID);
    assert_eq!(result.model, "omniroute/model-a");
    assert_eq!(result.text, "omniroute-output-sentinel");
    assert_eq!(result.usage, None);
    assert_eq!(store.retrieves.load(Ordering::SeqCst), 1);
    let debug = format!("{adapter:?} {:?}", request());
    for sentinel in [
        "omniroute-api-key-sentinel",
        "omniroute-system-sentinel",
        "omniroute-prompt-sentinel",
        "omniroute-output-sentinel",
    ] {
        assert!(!debug.contains(sentinel), "leaked {sentinel}");
    }
}

#[tokio::test]
async fn selection_requires_named_connection_and_known_model() {
    let mut registry = OmniRouteRegistry::default();
    registry.insert(connection()).expect("registry");
    let selection = ProviderSelection::new(
        "omniroute-local",
        OMNIROUTE_PROVIDER_ID,
        "omniroute/model-a",
    )
    .expect("selection");
    assert_eq!(
        selection.resolve(&registry).expect("resolve").connection_id,
        "omniroute-local"
    );
    assert_eq!(
        ProviderSelection::new("missing", OMNIROUTE_PROVIDER_ID, "omniroute/model-a")
            .expect("selection")
            .resolve(&registry),
        Err(ProviderSelectionError::ConnectionNotFound)
    );
    assert_eq!(
        ProviderSelection::new("omniroute-local", OMNIROUTE_PROVIDER_ID, "missing")
            .expect("selection")
            .resolve(&registry),
        Err(ProviderSelectionError::ModelNotFound)
    );
}
