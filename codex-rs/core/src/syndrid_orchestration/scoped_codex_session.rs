use super::codex_accounts::CodexCredentialEnvelope;
use super::codex_accounts::CodexCredentialSnapshot;
use super::invocation::ProviderInvocationError;
use super::invocation::ProviderInvocationRequest;
use super::invocation::ProviderInvocationResult;
use codex_api::ApiError;
use codex_api::ReqwestTransport;
use codex_api::ResponseEvent;
use codex_api::ResponsesApiRequest;
use codex_api::ResponsesClient;
use codex_api::ResponsesOptions;
use codex_model_provider::BearerAuthProvider;
use codex_model_provider_info::CHATGPT_CODEX_BASE_URL;
use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::auth::AuthMode;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use futures::StreamExt;
use reqwest::redirect::Policy;
use std::fmt;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const INVOCATION_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_OUTPUT_BYTES_PER_TOKEN: usize = 4;

/// A request-scoped Codex client using only the selected account's credentials.
///
/// The lower-level Responses client accepts an explicit auth provider, so this session never
/// constructs or consults an `AuthManager` and never writes authentication state to disk.
pub struct ScopedCodexSession {
    client: ResponsesClient<ReqwestTransport>,
    connection_id: String,
    account_id: Option<String>,
}

impl fmt::Debug for ScopedCodexSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ScopedCodexSession")
            .field("connection_id", &self.connection_id)
            .field("has_account_id", &self.account_id.is_some())
            .finish()
    }
}

impl ScopedCodexSession {
    pub fn new(
        connection_id: impl Into<String>,
        credential: &CodexCredentialEnvelope,
    ) -> Result<Self, ProviderInvocationError> {
        Self::new_with_base_url(connection_id, credential, CHATGPT_CODEX_BASE_URL)
    }

    pub(crate) fn new_with_base_url(
        connection_id: impl Into<String>,
        credential: &CodexCredentialEnvelope,
        base_url: &str,
    ) -> Result<Self, ProviderInvocationError> {
        let connection_id = connection_id.into();
        let snapshot: CodexCredentialSnapshot = credential.snapshot();
        let provider = ModelProviderInfo::create_openai_provider(Some(base_url.to_string()))
            .to_api_provider(Some(AuthMode::Headers))
            .map_err(|_| ProviderInvocationError::ScopedSessionConstructionFailed)?;
        let auth = Arc::new(BearerAuthProvider {
            token: Some(snapshot.access_token().to_string()),
            account_id: snapshot.account_id().map(str::to_string),
            is_fedramp_account: false,
        });
        let http_client = reqwest::Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(|_| ProviderInvocationError::ScopedSessionConstructionFailed)?;
        Ok(Self {
            client: ResponsesClient::new(ReqwestTransport::new(http_client), provider, auth),
            connection_id,
            account_id: snapshot.account_id().map(str::to_string),
        })
    }

    pub async fn invoke(
        &self,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> Result<ProviderInvocationResult, ProviderInvocationError> {
        if request.model.trim().is_empty() {
            return Err(ProviderInvocationError::InvalidModelId);
        }
        if request.user.trim().is_empty() || request.max_output_tokens == 0 {
            return Err(ProviderInvocationError::InvalidRequest);
        }
        let input = ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: request.user }],
            phase: None,
            internal_chat_message_metadata_passthrough: None,
        };
        let api_request = ResponsesApiRequest {
            model: request.model.clone(),
            instructions: request.system.unwrap_or_default(),
            input: vec![input],
            tools: None,
            tool_choice: "none".to_string(),
            parallel_tool_calls: false,
            reasoning: None,
            store: false,
            stream: true,
            stream_options: None,
            include: Vec::new(),
            service_tier: None,
            prompt_cache_key: None,
            text: None,
            client_metadata: None,
        };
        let stream = tokio::time::timeout(
            INVOCATION_TIMEOUT,
            self.client
                .stream_request(api_request, ResponsesOptions::default()),
        )
        .await
        .map_err(|_| ProviderInvocationError::RequestTimedOut)?
        .map_err(map_api_error)?;
        collect_response(
            stream,
            request.provider,
            request.model,
            request.max_output_tokens,
            self.account_id.clone(),
            cancellation,
        )
        .await
    }
}

async fn collect_response(
    mut stream: codex_api::ResponseStream,
    provider: String,
    model: String,
    max_output_tokens: u32,
    _account_id: Option<String>,
    cancellation: CancellationToken,
) -> Result<ProviderInvocationResult, ProviderInvocationError> {
    let mut text = String::new();
    let usage;
    let mut request_id = stream.upstream_request_id.take();
    loop {
        let event = tokio::select! {
            _ = cancellation.cancelled() => return Err(ProviderInvocationError::Cancelled),
            event = stream.next() => event,
        };
        let Some(event) = event else {
            return Err(ProviderInvocationError::StreamTerminated);
        };
        match event.map_err(map_api_error)? {
            ResponseEvent::OutputTextDelta(delta) => {
                text.push_str(&delta);
                if text.len() > max_output_tokens as usize * MAX_OUTPUT_BYTES_PER_TOKEN {
                    return Err(ProviderInvocationError::ResponseTooLarge);
                }
            }
            ResponseEvent::Completed {
                response_id,
                token_usage,
                ..
            } => {
                if request_id.is_none() {
                    request_id = Some(response_id);
                }
                usage = token_usage.map(|value| super::invocation::ProviderInvocationUsage {
                    input_tokens: u64::try_from(value.input_tokens).ok(),
                    output_tokens: u64::try_from(value.output_tokens).ok(),
                    total_tokens: u64::try_from(value.total_tokens).ok(),
                });
                break;
            }
            _ => {}
        }
    }
    if text.trim().is_empty() {
        return Err(ProviderInvocationError::MissingOutput);
    }
    Ok(ProviderInvocationResult {
        provider,
        model,
        text,
        finish_reason: Some("completed".to_string()),
        usage,
        request_id,
    })
}

fn map_api_error(error: ApiError) -> ProviderInvocationError {
    match error {
        ApiError::Transport(codex_api::TransportError::Timeout) => {
            ProviderInvocationError::RequestTimedOut
        }
        ApiError::Transport(codex_api::TransportError::Http { status, .. })
        | ApiError::Api { status, .. } => match status.as_u16() {
            401 => ProviderInvocationError::ReauthenticationRequired,
            402 => ProviderInvocationError::PaymentRequired,
            403 => ProviderInvocationError::Forbidden,
            429 => ProviderInvocationError::RateLimited,
            500..=599 => ProviderInvocationError::ProviderUnavailable,
            _ => ProviderInvocationError::ProviderRejected,
        },
        ApiError::RateLimit(_) | ApiError::QuotaExceeded => ProviderInvocationError::RateLimited,
        ApiError::Transport(_) | ApiError::Retryable { .. } | ApiError::ServerOverloaded => {
            ProviderInvocationError::ProviderUnavailable
        }
        ApiError::Stream(_) | ApiError::InvalidRequest { .. } | ApiError::CyberPolicy { .. } => {
            ProviderInvocationError::InvalidResponse
        }
        ApiError::ContextWindowExceeded | ApiError::UsageNotIncluded => {
            ProviderInvocationError::ProviderRejected
        }
    }
}

/// Provider-neutral client implementation for one scoped Codex session.
#[derive(Clone, Copy, Debug, Default)]
pub struct ScopedCodexInvocationClient;

impl super::codex_invocation::CodexInvocationClient for ScopedCodexInvocationClient {
    fn invoke(
        &self,
        credential: &CodexCredentialEnvelope,
        request: ProviderInvocationRequest,
        cancellation: CancellationToken,
    ) -> impl Future<Output = Result<ProviderInvocationResult, ProviderInvocationError>> + Send
    {
        let session = ScopedCodexSession::new("selected", credential);
        async move {
            let session = session?;
            session.invoke(request, cancellation).await
        }
    }
}
