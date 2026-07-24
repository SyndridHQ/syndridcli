use super::codex_accounts::CodexAccountProfileRegistry;
use super::codex_accounts::CodexAccountProfileState;
use super::codex_accounts::CodexCredentialEnvelope;
use super::codex_accounts::retrieve_codex_envelope;
use super::invocation::ProviderInvocation;
use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use super::omniroute::ProviderSelection;
use std::future::Future;
use tokio_util::sync::CancellationToken;

pub const CODEX_PROVIDER_ID: &str = "codex";

/// Resolves the credential for one exact Codex connection at the authentication boundary.
pub trait CodexCredentialProvider: Send + Sync {
    fn retrieve(
        &self,
        connection_id: &str,
    ) -> Result<CodexCredentialEnvelope, ProviderInvocationError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeCodexCredentialProvider;

impl CodexCredentialProvider for NativeCodexCredentialProvider {
    fn retrieve(
        &self,
        connection_id: &str,
    ) -> Result<CodexCredentialEnvelope, ProviderInvocationError> {
        retrieve_codex_envelope(connection_id).map_err(|error| {
            match error {
            super::codex_accounts::CodexAccountProfileError::CredentialStoreRejected => {
                ProviderInvocationError::CredentialStoreUnavailable
            }
            super::codex_accounts::CodexAccountProfileError::InvalidCredentialEnvelope
            | super::codex_accounts::CodexAccountProfileError::UnsupportedCredentialEnvelopeVersion
            | super::codex_accounts::CodexAccountProfileError::CredentialEnvelopeTooLarge
            | super::codex_accounts::CodexAccountProfileError::MissingRequiredCredentialField => {
                ProviderInvocationError::InvalidResponse
            }
            _ => ProviderInvocationError::CredentialNotFound,
        }
        })
    }
}

/// Narrow client boundary for a selected Codex account.
///
/// A production client must use only the supplied envelope and must not consult global Codex
/// authentication state. The current repository client does not expose that scoped operation, so
/// the default implementation deliberately reports live unavailability.
pub trait CodexInvocationClient: Send + Sync {
    fn invoke(
        &self,
        credential: &CodexCredentialEnvelope,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct UnavailableCodexInvocationClient;

impl CodexInvocationClient for UnavailableCodexInvocationClient {
    fn invoke(
        &self,
        _credential: &CodexCredentialEnvelope,
        _request: ProviderInvocationRequest,
        _cancellation: CancellationToken,
    ) -> impl Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send
    {
        async { Err(ProviderInvocationError::LiveCodexInvocationUnavailable) }
    }
}

/// Provider-neutral adapter bound to one exact Codex connection selection.
#[derive(Clone, Debug)]
pub struct CodexInvocationAdapter<C, S = NativeCodexCredentialProvider> {
    selection: ProviderSelection,
    accounts: CodexAccountProfileRegistry,
    client: C,
    credentials: S,
}

impl<C> CodexInvocationAdapter<C, NativeCodexCredentialProvider> {
    pub fn new(
        selection: ProviderSelection,
        accounts: CodexAccountProfileRegistry,
        client: C,
    ) -> Result<Self, ProviderInvocationError> {
        if selection.provider_id != CODEX_PROVIDER_ID {
            return Err(ProviderInvocationError::UnsupportedProvider);
        }
        if selection.model_id.trim().is_empty() {
            return Err(ProviderInvocationError::InvalidModelId);
        }
        Ok(Self::with_credential_provider(
            selection,
            accounts,
            NativeCodexCredentialProvider,
            client,
        ))
    }
}

impl<C, S> CodexInvocationAdapter<C, S> {
    pub fn with_credential_provider(
        selection: ProviderSelection,
        accounts: CodexAccountProfileRegistry,
        credentials: S,
        client: C,
    ) -> Self {
        Self {
            selection,
            accounts,
            client,
            credentials,
        }
    }
}

impl<C: CodexInvocationClient, S: CodexCredentialProvider> ProviderInvocation
    for CodexInvocationAdapter<C, S>
{
    fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send
    {
        async move {
            if request.provider != CODEX_PROVIDER_ID {
                return Err(ProviderInvocationError::UnsupportedProvider);
            }
            if request.model != self.selection.model_id {
                return Err(ProviderInvocationError::InvalidModelId);
            }
            let account = self
                .accounts
                .get_connection(&self.selection.connection_id)
                .ok_or(ProviderInvocationError::ConnectionUnvalidated)?;
            if account.provider_id != CODEX_PROVIDER_ID {
                return Err(ProviderInvocationError::UnsupportedProvider);
            }
            if !account.enabled || account.state == CodexAccountProfileState::Disabled {
                return Err(ProviderInvocationError::ConnectionDisabled);
            }
            if account.state != CodexAccountProfileState::Connected {
                return Err(ProviderInvocationError::ConnectionUnvalidated);
            }
            if account.credential_reference.trim().is_empty() {
                return Err(ProviderInvocationError::MissingCredentialReference);
            }
            let credential = self.credentials.retrieve(&self.selection.connection_id)?;
            self.client.invoke(&credential, request, cancellation).await
        }
    }
}

#[cfg(test)]
#[path = "codex_invocation_tests.rs"]
mod codex_invocation_tests;
