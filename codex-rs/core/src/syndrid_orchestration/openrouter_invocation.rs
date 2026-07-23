use super::credential_store::CredentialStore;
use super::credential_store::CredentialStoreError;
use super::invocation::ProviderInvocation;
use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use super::invocation::ProviderInvocationUsage;
use super::native_credential_store::NativeCredentialStore;
use super::openai_compatible::EndpointPolicy;
use super::openai_compatible::OpenAiCompatibleRequest;
use super::openai_compatible::OpenAiCompatibleTransport;
use super::openai_compatible::OpenAiCompatibleTransportError;
use super::openai_compatible::ReqwestOpenAiCompatibleTransport;
use super::provider_connection::AuthenticationMethod;
use super::provider_connection::ConnectionValidationStatus;
use super::provider_connection::ProviderConnection;
use std::fmt;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const OPENROUTER_PROVIDER_ID: &str = "openrouter";
const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/v1";
const OPENROUTER_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const DEFAULT_INVOCATION_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpenRouterInvocationConfiguration {
    pub(super) model_id: String,
    pub(super) timeout: Duration,
    pub(super) max_response_bytes: usize,
}

impl OpenRouterInvocationConfiguration {
    pub(super) fn new(model_id: impl Into<String>) -> Result<Self, ProviderInvocationError> {
        let model_id = model_id.into();
        if model_id.trim().is_empty() || model_id.len() > 256 {
            return Err(ProviderInvocationError::InvalidModelId);
        }
        Ok(Self {
            model_id,
            timeout: DEFAULT_INVOCATION_TIMEOUT,
            max_response_bytes: MAX_RESPONSE_BYTES,
        })
    }

    fn transport(&self) -> Result<ReqwestOpenAiCompatibleTransport, ProviderInvocationError> {
        ReqwestOpenAiCompatibleTransport::new(
            OPENROUTER_BASE_URL,
            OPENROUTER_CHAT_COMPLETIONS_PATH,
            EndpointPolicy::HttpsOnly,
            self.timeout,
            self.max_response_bytes,
        )
        .map_err(map_transport_error)
    }
}

pub(super) struct OpenRouterInvocationAdapter<S, T> {
    connection: ProviderConnection,
    store: S,
    transport: T,
    configuration: OpenRouterInvocationConfiguration,
}

impl<S, T> fmt::Debug for OpenRouterInvocationAdapter<S, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenRouterInvocationAdapter")
            .field("provider", &OPENROUTER_PROVIDER_ID)
            .field("model_id", &self.configuration.model_id)
            .field("timeout", &self.configuration.timeout)
            .field("connection_id", &self.connection.connection_id)
            .finish()
    }
}

impl<S, T> OpenRouterInvocationAdapter<S, T> {
    pub(super) fn new(
        connection: ProviderConnection,
        store: S,
        transport: T,
        configuration: OpenRouterInvocationConfiguration,
    ) -> Self {
        Self {
            connection,
            store,
            transport,
            configuration,
        }
    }
}

impl<S> OpenRouterInvocationAdapter<S, ReqwestOpenAiCompatibleTransport>
where
    S: CredentialStore,
{
    pub(super) fn from_native_store(
        connection: ProviderConnection,
        store: S,
        model_id: impl Into<String>,
    ) -> Result<Self, ProviderInvocationError> {
        let configuration = OpenRouterInvocationConfiguration::new(model_id)?;
        let transport = configuration.transport()?;
        Ok(Self::new(connection, store, transport, configuration))
    }
}

impl<S, T> ProviderInvocation for OpenRouterInvocationAdapter<S, T>
where
    S: CredentialStore,
    T: OpenAiCompatibleTransport,
{
    async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        validate_connection(&self.connection)?;
        if request.provider != OPENROUTER_PROVIDER_ID {
            return Err(ProviderInvocationError::UnsupportedProvider);
        }
        if request.model != self.configuration.model_id {
            return Err(ProviderInvocationError::InvalidModelId);
        }
        let credential_reference = self
            .connection
            .credential_reference
            .as_ref()
            .ok_or(ProviderInvocationError::MissingCredentialReference)?;
        let credential = self
            .store
            .retrieve(credential_reference)
            .map_err(map_store_error)?;
        let provider_request = OpenAiCompatibleRequest::new(
            request.model.clone(),
            request.system,
            request.user,
            request.max_output_tokens,
        )
        .map_err(map_transport_error)?;
        let response = self
            .transport
            .invoke(credential.expose_for_auth(), provider_request, cancellation)
            .await
            .map_err(map_transport_error)?;
        Ok(ProviderInvocationResult {
            provider: OPENROUTER_PROVIDER_ID.to_string(),
            model: response.model.unwrap_or(request.model),
            text: response.text,
            finish_reason: response.finish_reason,
            usage: response.usage.map(|usage| ProviderInvocationUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                total_tokens: usage.total_tokens,
            }),
            request_id: response.request_id,
        })
    }
}

fn validate_connection(connection: &ProviderConnection) -> Result<(), ProviderInvocationError> {
    if connection.provider_id.as_str() != OPENROUTER_PROVIDER_ID {
        return Err(ProviderInvocationError::UnsupportedProvider);
    }
    if connection.authentication_method != AuthenticationMethod::OAuthPkce {
        return Err(ProviderInvocationError::UnsupportedAuthenticationMethod);
    }
    if !connection.enabled {
        return Err(ProviderInvocationError::ConnectionDisabled);
    }
    if connection.validation.status != ConnectionValidationStatus::Valid {
        return Err(ProviderInvocationError::ConnectionUnvalidated);
    }
    Ok(())
}

fn map_store_error(error: CredentialStoreError) -> ProviderInvocationError {
    match error {
        CredentialStoreError::NotFound => ProviderInvocationError::CredentialNotFound,
        CredentialStoreError::Unavailable => ProviderInvocationError::CredentialStoreUnavailable,
        CredentialStoreError::InvalidReference | CredentialStoreError::Rejected => {
            ProviderInvocationError::CredentialStoreRejected
        }
    }
}

fn map_transport_error(error: OpenAiCompatibleTransportError) -> ProviderInvocationError {
    match error {
        OpenAiCompatibleTransportError::InvalidConfiguration => {
            ProviderInvocationError::InvalidConfiguration
        }
        OpenAiCompatibleTransportError::InvalidRequest => ProviderInvocationError::InvalidRequest,
        OpenAiCompatibleTransportError::InputTooLarge => ProviderInvocationError::InputTooLarge,
        OpenAiCompatibleTransportError::OutputLimitInvalid => {
            ProviderInvocationError::OutputLimitInvalid
        }
        OpenAiCompatibleTransportError::TransportUnavailable => {
            ProviderInvocationError::TransportUnavailable
        }
        OpenAiCompatibleTransportError::RequestTimedOut => ProviderInvocationError::RequestTimedOut,
        OpenAiCompatibleTransportError::Cancelled => ProviderInvocationError::Cancelled,
        OpenAiCompatibleTransportError::Unauthorized => ProviderInvocationError::Unauthorized,
        OpenAiCompatibleTransportError::PaymentRequired => ProviderInvocationError::PaymentRequired,
        OpenAiCompatibleTransportError::Forbidden => ProviderInvocationError::Forbidden,
        OpenAiCompatibleTransportError::RateLimited { .. } => ProviderInvocationError::RateLimited,
        OpenAiCompatibleTransportError::ProviderUnavailable => {
            ProviderInvocationError::ProviderUnavailable
        }
        OpenAiCompatibleTransportError::ProviderRejected => {
            ProviderInvocationError::ProviderRejected
        }
        OpenAiCompatibleTransportError::InvalidContentType => {
            ProviderInvocationError::InvalidContentType
        }
        OpenAiCompatibleTransportError::ResponseTooLarge => {
            ProviderInvocationError::ResponseTooLarge
        }
        OpenAiCompatibleTransportError::InvalidResponse => ProviderInvocationError::InvalidResponse,
        OpenAiCompatibleTransportError::MissingOutput => ProviderInvocationError::MissingOutput,
    }
}

pub(super) fn native_openrouter_adapter(
    connection: ProviderConnection,
    model_id: impl Into<String>,
) -> Result<
    OpenRouterInvocationAdapter<NativeCredentialStore, ReqwestOpenAiCompatibleTransport>,
    ProviderInvocationError,
> {
    OpenRouterInvocationAdapter::from_native_store(
        connection,
        NativeCredentialStore::new(),
        model_id,
    )
}

#[cfg(test)]
#[path = "openrouter_invocation_tests.rs"]
mod tests;
