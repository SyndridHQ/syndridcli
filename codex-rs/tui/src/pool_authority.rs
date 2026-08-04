//! TUI-owned access to the canonical named-pool registry.

use crate::legacy_core::AccountPoolError;
use crate::legacy_core::CodexAccountProfileRegistry;
use crate::legacy_core::NamedAccountPoolRegistry;
use crate::legacy_core::NamedAccountPoolStore;
use crate::legacy_core::OmniRouteRegistry;
use std::path::Path;
use std::sync::Arc;
use std::sync::RwLock;

pub(crate) trait PoolRegistryWriter: Send + Sync {
    fn save(&self, registry: &NamedAccountPoolRegistry) -> Result<(), AccountPoolError>;
}

impl PoolRegistryWriter for NamedAccountPoolStore {
    fn save(&self, registry: &NamedAccountPoolRegistry) -> Result<(), AccountPoolError> {
        NamedAccountPoolStore::save(self, registry)
    }
}

/// The single TUI access point for the active canonical pool registry.
pub(crate) struct TuiPoolAuthority {
    pub(crate) registry: Arc<RwLock<NamedAccountPoolRegistry>>,
    pub(crate) accounts: Option<Arc<CodexAccountProfileRegistry>>,
    pub(crate) omni_route: Option<Arc<OmniRouteRegistry>>,
    store: Arc<dyn PoolRegistryWriter>,
    load_error: Arc<RwLock<Option<AccountPoolError>>>,
}

impl TuiPoolAuthority {
    pub(crate) fn load(
        codex_home: &Path,
        accounts: Option<Arc<CodexAccountProfileRegistry>>,
        omni_route: Option<Arc<OmniRouteRegistry>>,
    ) -> Self {
        let path = codex_home.join(crate::legacy_core::ACCOUNT_POOL_FILE);
        let store = NamedAccountPoolStore::new(path);
        let file_exists = store.path().exists();
        let (registry, load_error) = match store.load() {
            Ok(registry) => (registry, None),
            Err(error) => (
                NamedAccountPoolRegistry::default(),
                file_exists.then_some(error),
            ),
        };
        Self {
            registry: Arc::new(RwLock::new(registry)),
            accounts,
            omni_route,
            store: Arc::new(store),
            load_error: Arc::new(RwLock::new(load_error)),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        registry: NamedAccountPoolRegistry,
        writer: Arc<dyn PoolRegistryWriter>,
    ) -> Self {
        Self {
            registry: Arc::new(RwLock::new(registry)),
            accounts: None,
            omni_route: None,
            store: writer,
            load_error: Arc::new(RwLock::new(None)),
        }
    }

    pub(crate) fn candidate(&self) -> Option<NamedAccountPoolRegistry> {
        self.registry.read().ok().map(|registry| registry.clone())
    }

    pub(crate) fn load_error(&self) -> Option<AccountPoolError> {
        self.load_error.read().ok().and_then(|error| *error)
    }

    pub(crate) fn save(
        &self,
        candidate: &NamedAccountPoolRegistry,
    ) -> Result<(), AccountPoolError> {
        if self.load_error().is_some() {
            return Err(AccountPoolError::RegistryMalformed);
        }
        self.store.save(candidate)?;
        *self
            .registry
            .write()
            .map_err(|_| AccountPoolError::AtomicWriteFailed)? = candidate.clone();
        *self
            .load_error
            .write()
            .map_err(|_| AccountPoolError::AtomicWriteFailed)? = None;
        Ok(())
    }

    pub(crate) fn replace_invalid(
        &self,
        candidate: &NamedAccountPoolRegistry,
    ) -> Result<(), AccountPoolError> {
        self.store.save(candidate)?;
        *self
            .registry
            .write()
            .map_err(|_| AccountPoolError::AtomicWriteFailed)? = candidate.clone();
        *self
            .load_error
            .write()
            .map_err(|_| AccountPoolError::AtomicWriteFailed)? = None;
        Ok(())
    }
}
