use super::super::credential_store::CredentialStore;
use super::super::credential_store::CredentialStoreError;
use super::KeyringCredentialStore;
use super::NativeCredentialStore;
use super::SERVICE_NAMESPACE;
use super::native_keyring_coordinates;
use crate::syndrid_orchestration::provider_connection::CredentialReference;
use crate::syndrid_orchestration::provider_connection::CredentialSecret;
use codex_keyring_store::KeyringStore;
use codex_keyring_store::tests::MockKeyringStore;
use keyring::Error as KeyringError;

const SENTINEL: &str = "SYNDIRD_SECRET_MUST_NEVER_APPEAR_9f3d";

fn reference(value: &str) -> CredentialReference {
    CredentialReference::new(value).expect("valid credential reference")
}

fn store() -> (MockKeyringStore, KeyringCredentialStore<MockKeyringStore>) {
    let backend = MockKeyringStore::default();
    let store = KeyringCredentialStore::with_backend(backend.clone());
    (backend, store)
}

#[derive(Clone, Copy, Debug)]
enum FailureKind {
    Unavailable,
    Rejected,
}

#[derive(Clone, Copy, Debug)]
struct FailingBackend {
    kind: FailureKind,
}

impl FailingBackend {
    fn error(self) -> codex_keyring_store::CredentialStoreError {
        let error = match self.kind {
            FailureKind::Unavailable => KeyringError::NoStorageAccess(Box::new(
                std::io::Error::other("test backend unavailable"),
            )),
            FailureKind::Rejected => {
                KeyringError::Invalid("test attribute".to_string(), "test rejection".to_string())
            }
        };
        codex_keyring_store::CredentialStoreError::new(error)
    }
}

impl KeyringStore for FailingBackend {
    fn load(
        &self,
        _service: &str,
        _account: &str,
    ) -> Result<Option<String>, codex_keyring_store::CredentialStoreError> {
        Err(self.error())
    }

    fn save(
        &self,
        _service: &str,
        _account: &str,
        _value: &str,
    ) -> Result<(), codex_keyring_store::CredentialStoreError> {
        Err(self.error())
    }

    fn delete(
        &self,
        _service: &str,
        _account: &str,
    ) -> Result<bool, codex_keyring_store::CredentialStoreError> {
        Err(self.error())
    }
}

#[test]
fn references_map_to_stable_bounded_native_coordinates() {
    let first = native_keyring_coordinates(&reference("credential-a"));
    let second = native_keyring_coordinates(&reference("credential-a"));
    assert_eq!(first, second);
    assert_eq!(first.service, SERVICE_NAMESPACE);
    assert!(first.account.starts_with("syndrid-credential-"));
    assert!(first.account.len() <= 128);
    assert_ne!(
        first,
        native_keyring_coordinates(&reference("credential-b"))
    );
}

#[test]
fn coordinates_do_not_include_secret_material() {
    let coordinates = native_keyring_coordinates(&reference("credential-a"));
    assert!(!coordinates.service.contains(SENTINEL));
    assert!(!coordinates.account.contains(SENTINEL));
    assert!(!coordinates.account.contains("credential-a"));
}

#[test]
fn lifecycle_operations_use_the_native_backend_without_serializing_secrets() {
    let (backend, store) = store();
    let credential_reference = reference("credential-lifecycle");
    let secret = CredentialSecret::new(SENTINEL.to_string()).expect("secret");
    let coordinates = native_keyring_coordinates(&credential_reference);

    assert!(!store.contains(&credential_reference).expect("contains"));
    store
        .store(&credential_reference, secret)
        .expect("store credential");
    assert_eq!(
        backend.saved_value(&coordinates.account).as_deref(),
        Some(SENTINEL)
    );
    assert!(store.contains(&credential_reference).expect("contains"));
    let retrieved = store
        .retrieve(&credential_reference)
        .expect("retrieve credential");
    assert_eq!(retrieved.expose_for_auth(), SENTINEL);
    assert!(!format!("{retrieved:?}").contains(SENTINEL));
    assert!(!format!("{retrieved}").contains(SENTINEL));
    store
        .delete(&credential_reference)
        .expect("delete credential");
    assert!(!store.contains(&credential_reference).expect("contains"));
}

#[test]
fn missing_entries_map_to_not_found() {
    let (_, store) = store();
    let credential_reference = reference("missing");
    assert!(matches!(
        store.retrieve(&credential_reference),
        Err(CredentialStoreError::NotFound)
    ));
    assert_eq!(
        store.delete(&credential_reference),
        Err(CredentialStoreError::NotFound)
    );
}

#[test]
fn native_backend_failures_map_to_bounded_errors() {
    let unavailable_store = KeyringCredentialStore::with_backend(FailingBackend {
        kind: FailureKind::Unavailable,
    });
    let unavailable_reference = reference("unavailable");
    assert!(matches!(
        unavailable_store.retrieve(&unavailable_reference),
        Err(CredentialStoreError::Unavailable)
    ));
    assert!(matches!(
        unavailable_store.contains(&unavailable_reference),
        Err(CredentialStoreError::Unavailable)
    ));

    let rejected_store = KeyringCredentialStore::with_backend(FailingBackend {
        kind: FailureKind::Rejected,
    });
    let rejected_reference = reference("rejected");
    assert!(matches!(
        rejected_store.retrieve(&rejected_reference),
        Err(CredentialStoreError::Rejected)
    ));
    assert!(matches!(
        rejected_store.contains(&rejected_reference),
        Err(CredentialStoreError::Rejected)
    ));
    assert!(matches!(
        rejected_store.store(
            &rejected_reference,
            CredentialSecret::new("rejected-secret").expect("secret")
        ),
        Err(CredentialStoreError::Rejected)
    ));
    assert!(matches!(
        rejected_store.delete(&rejected_reference),
        Err(CredentialStoreError::Rejected)
    ));
    assert!(!format!("{:?}", CredentialStoreError::Rejected).contains(SENTINEL));
    assert!(
        !CredentialStoreError::Rejected
            .to_string()
            .contains(SENTINEL)
    );
}

#[test]
fn production_store_has_a_native_backend_constructor() {
    let _store = NativeCredentialStore::new();
}
