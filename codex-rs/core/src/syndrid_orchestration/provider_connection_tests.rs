use super::super::credential_store::CredentialStore;
use super::super::credential_store::CredentialStoreError;
use super::*;
const SENTINEL: &str = "SYNDIRD_SECRET_MUST_NEVER_APPEAR_9f3d";

fn provider_connection(
    method: AuthenticationMethod,
    reference: Option<CredentialReference>,
) -> Result<ProviderConnection, ProviderConnectionError> {
    ProviderConnection::new(
        ProviderConnectionId::new("connection-1")?,
        ProviderId::new("provider-a")?,
        ConnectionLabel::new("Provider A")?,
        method,
        reference,
        None,
        true,
    )
}

fn api_key_connection() -> ProviderConnection {
    provider_connection(
        AuthenticationMethod::ApiKey,
        Some(CredentialReference::new("credential-1").expect("credential reference")),
    )
    .expect("valid API-key connection")
}

#[test]
fn valid_api_key_connection_metadata_accepts_opaque_reference() {
    let connection = api_key_connection();
    assert_eq!(
        connection.validation.status,
        ConnectionValidationStatus::Unvalidated
    );
    assert_ne!(
        connection.validation.status,
        ConnectionValidationStatus::Valid
    );
    assert_eq!(connection.provider_id.as_str(), "provider-a");
    assert_eq!(
        connection
            .credential_reference
            .as_ref()
            .expect("credential reference")
            .as_str(),
        "credential-1"
    );
}

#[test]
fn authentication_credential_combinations_are_enforced() {
    assert_eq!(
        provider_connection(AuthenticationMethod::ApiKey, None),
        Err(ProviderConnectionError::CredentialRequired)
    );
    assert_eq!(
        provider_connection(
            AuthenticationMethod::NoAuthentication,
            Some(CredentialReference::new("ref").expect("reference"))
        ),
        Err(ProviderConnectionError::CredentialForbidden)
    );
    assert!(provider_connection(AuthenticationMethod::LocalEndpoint, None).is_ok());
    assert!(
        provider_connection(
            AuthenticationMethod::OAuth,
            Some(CredentialReference::new("ref").expect("reference"))
        )
        .is_ok()
    );
    assert!(
        provider_connection(
            AuthenticationMethod::OAuthPkce,
            Some(CredentialReference::new("ref").expect("reference"))
        )
        .is_ok()
    );
}

#[test]
fn bounded_metadata_rejects_empty_whitespace_and_oversized_values() {
    assert!(ProviderId::new("").is_err());
    assert!(ProviderConnectionId::new(" \t").is_err());
    assert!(ConnectionLabel::new("\n").is_err());
    assert!(CredentialReference::new("").is_err());
    assert!(ProviderId::new("x".repeat(129)).is_err());
    assert!(ProviderConnectionId::new("x".repeat(129)).is_err());
    assert!(ConnectionLabel::new("x".repeat(257)).is_err());
    assert!(CredentialReference::new("x".repeat(129)).is_err());
    assert!(CredentialSecret::new(SENTINEL.to_string()).is_ok());
    assert!(CredentialSecret::new(" ".to_string()).is_err());
    assert!(CredentialSecret::new("x".repeat(65 * 1024)).is_err());
}

#[test]
fn endpoint_metadata_is_bounded_and_does_not_accept_embedded_credentials() {
    assert!(EndpointUrl::new("http://localhost:11434").is_ok());
    assert!(EndpointUrl::new("not a URL").is_err());
    assert!(EndpointUrl::new("file:///tmp/model").is_err());
    assert!(EndpointUrl::new("https://user:password@example.com").is_err());
    assert!(EndpointUrl::new("https://example.com/?token=secret").is_err());
    assert!(EndpointUrl::new(format!("https://{}", "x".repeat(2041))).is_err());
}

#[test]
fn duplicate_connection_ids_are_rejected() {
    let mut registry = ProviderConnectionRegistry::default();
    registry
        .insert(api_key_connection())
        .expect("first connection");
    assert_eq!(
        registry.insert(api_key_connection()),
        Err(ProviderConnectionError::DuplicateConnectionId)
    );
    assert!(
        registry
            .get(&ProviderConnectionId::new("connection-1").expect("connection id"))
            .is_some()
    );
}

#[test]
fn secret_values_are_redacted_from_debug_display_and_connection_serialization() {
    let secret = CredentialSecret::new(SENTINEL.to_string()).expect("secret");
    assert!(!format!("{secret:?}").contains(SENTINEL));
    assert!(!format!("{secret}").contains(SENTINEL));

    let connection = api_key_connection();
    let debug = format!("{connection:?}");
    let serialized = serde_json::to_string(&connection).expect("connection serialization");
    assert!(!debug.contains(SENTINEL));
    assert!(!serialized.contains(SENTINEL));
    assert!(serialized.contains("credential-1"));
}

#[test]
fn credential_references_serialize_without_secret_material() {
    let reference = CredentialReference::new("credential-1").expect("reference");
    let serialized = serde_json::to_string(&reference).expect("reference serialization");
    assert_eq!(serialized, "\"credential-1\"");
    assert!(!serialized.contains(SENTINEL));
}

#[test]
fn credential_store_errors_never_echo_submitted_secret() {
    struct FailingStore;

    impl CredentialStore for FailingStore {
        fn store(
            &self,
            _reference: &CredentialReference,
            _secret: CredentialSecret,
        ) -> Result<(), CredentialStoreError> {
            Err(CredentialStoreError::Unavailable)
        }

        fn retrieve(
            &self,
            _reference: &CredentialReference,
        ) -> Result<CredentialSecret, CredentialStoreError> {
            Err(CredentialStoreError::Unavailable)
        }

        fn delete(&self, _reference: &CredentialReference) -> Result<(), CredentialStoreError> {
            Err(CredentialStoreError::Unavailable)
        }

        fn contains(&self, _reference: &CredentialReference) -> Result<bool, CredentialStoreError> {
            Err(CredentialStoreError::Unavailable)
        }
    }

    let reference = CredentialReference::new("credential-1").expect("reference");
    let error = FailingStore
        .store(
            &reference,
            CredentialSecret::new(SENTINEL.to_string()).expect("secret"),
        )
        .expect_err("store should fail");
    assert!(!error.to_string().contains(SENTINEL));
}

#[test]
fn validation_result_is_serializable_metadata_only() {
    let result = ConnectionValidationResult::valid();
    let serialized = serde_json::to_string(&result).expect("validation result serialization");
    assert!(serialized.contains("valid"));
    assert!(!serialized.contains(SENTINEL));
}
