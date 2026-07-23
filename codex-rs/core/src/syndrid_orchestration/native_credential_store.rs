use super::credential_store::CredentialStore;
use super::credential_store::CredentialStoreError;
use super::provider_connection::CredentialReference;
use super::provider_connection::CredentialSecret;
use codex_keyring_store::CredentialStoreErrorKind;
use codex_keyring_store::DefaultKeyringStore;
use codex_keyring_store::KeyringStore;
use sha2::Digest;
use sha2::Sha256;

const SERVICE_NAMESPACE: &str = "syndrid-provider-credentials";
const ACCOUNT_PREFIX: &str = "syndrid-credential-";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeKeyringCoordinates {
    pub(super) service: &'static str,
    pub(super) account: String,
}

pub(super) fn native_keyring_coordinates(
    reference: &CredentialReference,
) -> NativeKeyringCoordinates {
    let mut digest = Sha256::new();
    digest.update(b"syndrid:credential-reference:");
    digest.update(reference.as_str().as_bytes());
    NativeKeyringCoordinates {
        service: SERVICE_NAMESPACE,
        account: format!("{ACCOUNT_PREFIX}{:x}", digest.finalize()),
    }
}

pub(super) struct KeyringCredentialStore<K> {
    backend: K,
}

pub(super) type NativeCredentialStore = KeyringCredentialStore<DefaultKeyringStore>;

impl NativeCredentialStore {
    pub(super) fn new() -> Self {
        Self {
            backend: DefaultKeyringStore,
        }
    }
}

impl<K> KeyringCredentialStore<K> {
    pub(super) fn with_backend(backend: K) -> Self {
        Self { backend }
    }
}

impl<K: KeyringStore> CredentialStore for KeyringCredentialStore<K> {
    fn store(
        &self,
        reference: &CredentialReference,
        secret: CredentialSecret,
    ) -> Result<(), CredentialStoreError> {
        let coordinates = native_keyring_coordinates(reference);
        self.backend
            .save(
                coordinates.service,
                &coordinates.account,
                secret.expose_for_auth(),
            )
            .map_err(map_native_error)
    }

    fn retrieve(
        &self,
        reference: &CredentialReference,
    ) -> Result<CredentialSecret, CredentialStoreError> {
        let coordinates = native_keyring_coordinates(reference);
        let value = self
            .backend
            .load(coordinates.service, &coordinates.account)
            .map_err(map_native_error)?
            .ok_or(CredentialStoreError::NotFound)?;
        CredentialSecret::new(value).map_err(|_| CredentialStoreError::Rejected)
    }

    fn delete(&self, reference: &CredentialReference) -> Result<(), CredentialStoreError> {
        let coordinates = native_keyring_coordinates(reference);
        let removed = self
            .backend
            .delete(coordinates.service, &coordinates.account)
            .map_err(map_native_error)?;
        if removed {
            Ok(())
        } else {
            Err(CredentialStoreError::NotFound)
        }
    }

    fn contains(&self, reference: &CredentialReference) -> Result<bool, CredentialStoreError> {
        let coordinates = native_keyring_coordinates(reference);
        self.backend
            .load(coordinates.service, &coordinates.account)
            .map(|value| value.is_some())
            .map_err(map_native_error)
    }
}

fn map_native_error(error: codex_keyring_store::CredentialStoreError) -> CredentialStoreError {
    match error.kind() {
        CredentialStoreErrorKind::NotFound => CredentialStoreError::NotFound,
        CredentialStoreErrorKind::Unavailable => CredentialStoreError::Unavailable,
        CredentialStoreErrorKind::Rejected => CredentialStoreError::Rejected,
    }
}

#[cfg(test)]
#[path = "native_credential_store_tests.rs"]
mod native_credential_store_tests;
