use codex_http_client::ClientRouteClass;
use codex_http_client::HttpClient;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use url::Url;

const MAX_BASE_URL_BYTES: usize = 2048;
const MAX_ENDPOINT_PATH_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_REQUEST_BYTES: usize = 32 * 1024;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;
const MAX_OUTPUT_TOKENS: u32 = 16_384;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EndpointPolicy {
    HttpsOnly,
    LoopbackHttp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum OpenAiCompatibleTransportError {
    InvalidConfiguration,
    InvalidRequest,
    InputTooLarge,
    OutputLimitInvalid,
    TransportUnavailable,
    RequestTimedOut,
    Cancelled,
    Unauthorized,
    PaymentRequired,
    Forbidden,
    RateLimited { retry_after: Option<Duration> },
    ProviderUnavailable,
    ProviderRejected,
    InvalidContentType,
    ResponseTooLarge,
    InvalidResponse,
    MissingOutput,
}

impl fmt::Display for OpenAiCompatibleTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidConfiguration => "OpenAI-compatible endpoint configuration is invalid",
            Self::InvalidRequest => "OpenAI-compatible request is invalid",
            Self::InputTooLarge => "OpenAI-compatible request input is too large",
            Self::OutputLimitInvalid => "OpenAI-compatible output limit is invalid",
            Self::TransportUnavailable => "OpenAI-compatible transport is unavailable",
            Self::RequestTimedOut => "OpenAI-compatible request timed out",
            Self::Cancelled => "OpenAI-compatible request was cancelled",
            Self::Unauthorized => "provider authorization was rejected",
            Self::PaymentRequired => "provider payment is required",
            Self::Forbidden => "provider request was forbidden",
            Self::RateLimited { .. } => "provider rate limit was reached",
            Self::ProviderUnavailable => "provider is unavailable",
            Self::ProviderRejected => "provider rejected the request",
            Self::InvalidContentType => "provider response content type is invalid",
            Self::ResponseTooLarge => "provider response is too large",
            Self::InvalidResponse => "provider response is invalid",
            Self::MissingOutput => "provider response did not contain output",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for OpenAiCompatibleTransportError {}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct OpenAiCompatibleRequest {
    pub(super) model: String,
    pub(super) system: Option<String>,
    pub(super) user: String,
    pub(super) max_output_tokens: u32,
}

impl fmt::Debug for OpenAiCompatibleRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleRequest")
            .field("model", &self.model)
            .field("has_system", &self.system.is_some())
            .field("system_bytes", &self.system.as_ref().map_or(0, String::len))
            .field("user_input_bytes", &self.user.len())
            .field("max_output_tokens", &self.max_output_tokens)
            .finish()
    }
}

impl OpenAiCompatibleRequest {
    pub(super) fn new(
        model: impl Into<String>,
        system: Option<String>,
        user: impl Into<String>,
        max_output_tokens: u32,
    ) -> Result<Self, OpenAiCompatibleTransportError> {
        let model = model.into();
        let user = user.into();
        if model.trim().is_empty() || model.len() > MAX_MODEL_BYTES {
            return Err(OpenAiCompatibleTransportError::InvalidRequest);
        }
        if user.trim().is_empty() {
            return Err(OpenAiCompatibleTransportError::InvalidRequest);
        }
        if user.len() > MAX_MESSAGE_BYTES
            || system
                .as_ref()
                .is_some_and(|value| value.len() > MAX_MESSAGE_BYTES)
        {
            return Err(OpenAiCompatibleTransportError::InputTooLarge);
        }
        if !(1..=MAX_OUTPUT_TOKENS).contains(&max_output_tokens) {
            return Err(OpenAiCompatibleTransportError::OutputLimitInvalid);
        }
        Ok(Self {
            model,
            system,
            user,
            max_output_tokens,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpenAiCompatibleUsage {
    pub(super) input_tokens: Option<u64>,
    pub(super) output_tokens: Option<u64>,
    pub(super) total_tokens: Option<u64>,
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct OpenAiCompatibleResponse {
    pub(super) text: String,
    pub(super) model: Option<String>,
    pub(super) finish_reason: Option<String>,
    pub(super) usage: Option<OpenAiCompatibleUsage>,
    pub(super) request_id: Option<String>,
}

impl fmt::Debug for OpenAiCompatibleResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenAiCompatibleResponse")
            .field("model", &self.model)
            .field("generated_output_bytes", &self.text.len())
            .field("finish_reason", &self.finish_reason)
            .field("usage", &self.usage)
            .field("request_id", &self.request_id)
            .finish()
    }
}

pub(super) trait OpenAiCompatibleTransport: Send + Sync {
    fn invoke(
        &self,
        bearer: &str,
        request: OpenAiCompatibleRequest,
        cancellation: CancellationToken,
    ) -> impl std::future::Future<
        Output = Result<OpenAiCompatibleResponse, OpenAiCompatibleTransportError>,
    > + Send;
}

#[derive(Clone)]
pub(super) struct ReqwestOpenAiCompatibleTransport {
    client: HttpClient,
    endpoint: String,
    timeout: Duration,
    max_response_bytes: usize,
}

impl fmt::Debug for ReqwestOpenAiCompatibleTransport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReqwestOpenAiCompatibleTransport")
            .field("endpoint", &self.endpoint)
            .field("timeout", &self.timeout)
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

impl ReqwestOpenAiCompatibleTransport {
    pub(super) fn new(
        base_url: impl Into<String>,
        endpoint_path: impl Into<String>,
        policy: EndpointPolicy,
        timeout: Duration,
        max_response_bytes: usize,
    ) -> Result<Self, OpenAiCompatibleTransportError> {
        if timeout.is_zero() || max_response_bytes == 0 || max_response_bytes > MAX_RESPONSE_BYTES {
            return Err(OpenAiCompatibleTransportError::InvalidConfiguration);
        }
        let base_url = validate_base_url(base_url.into(), policy)?;
        let endpoint_path = endpoint_path.into();
        if endpoint_path.len() > MAX_ENDPOINT_PATH_BYTES
            || !endpoint_path.starts_with('/')
            || endpoint_path.contains('?')
            || endpoint_path.contains('#')
        {
            return Err(OpenAiCompatibleTransportError::InvalidConfiguration);
        }
        let endpoint = format!("{base_url}{endpoint_path}");
        let client = HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault)
            .build_reqwest_client(
                reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()),
                &endpoint,
                ClientRouteClass::Api,
            )
            .map(HttpClient::new)
            .map_err(|_| OpenAiCompatibleTransportError::TransportUnavailable)?;
        Ok(Self {
            client,
            endpoint,
            timeout,
            max_response_bytes,
        })
    }
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    max_tokens: u32,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'static str,
    content: &'a str,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    id: Option<String>,
    model: Option<String>,
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: Option<ChatMessageResponse>,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct ChatMessageResponse {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

impl OpenAiCompatibleTransport for ReqwestOpenAiCompatibleTransport {
    async fn invoke(
        &self,
        bearer: &str,
        request: OpenAiCompatibleRequest,
        cancellation: CancellationToken,
    ) -> Result<OpenAiCompatibleResponse, OpenAiCompatibleTransportError> {
        if bearer.trim().is_empty() {
            return Err(OpenAiCompatibleTransportError::InvalidRequest);
        }
        let body = serde_json::to_vec(&ChatCompletionRequest {
            model: &request.model,
            messages: match request.system.as_deref() {
                Some(system) => vec![
                    ChatMessage {
                        role: "system",
                        content: system,
                    },
                    ChatMessage {
                        role: "user",
                        content: &request.user,
                    },
                ],
                None => vec![ChatMessage {
                    role: "user",
                    content: &request.user,
                }],
            },
            max_tokens: request.max_output_tokens,
        })
        .map_err(|_| OpenAiCompatibleTransportError::InvalidRequest)?;
        if body.len() > MAX_REQUEST_BYTES {
            return Err(OpenAiCompatibleTransportError::InputTooLarge);
        }
        let request = self
            .client
            .post(&self.endpoint)
            .bearer_auth(bearer)
            .header(
                reqwest::header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            )
            .body(body)
            .timeout(self.timeout);
        let response = tokio::select! {
            _ = cancellation.cancelled() => return Err(OpenAiCompatibleTransportError::Cancelled),
            response = request.send() => response.map_err(|error| {
                if error.is_timeout() {
                    OpenAiCompatibleTransportError::RequestTimedOut
                } else {
                    OpenAiCompatibleTransportError::TransportUnavailable
                }
            })?,
        };
        let status = response.status();
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        let content_type_is_json = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"));
        let body = tokio::select! {
            _ = cancellation.cancelled() => return Err(OpenAiCompatibleTransportError::Cancelled),
            body = read_bounded_body(response, self.max_response_bytes) => body?,
        };
        if !status.is_success() {
            return Err(map_status(status, retry_after, &body));
        }
        if !content_type_is_json {
            return Err(OpenAiCompatibleTransportError::InvalidContentType);
        }
        let response: ChatCompletionResponse = serde_json::from_slice(&body)
            .map_err(|_| OpenAiCompatibleTransportError::InvalidResponse)?;
        let choice = response
            .choices
            .into_iter()
            .next()
            .ok_or(OpenAiCompatibleTransportError::MissingOutput)?;
        let message = choice
            .message
            .and_then(|message| message.content)
            .filter(|content| !content.trim().is_empty())
            .ok_or(OpenAiCompatibleTransportError::MissingOutput)?;
        if message.len() > MAX_MESSAGE_BYTES {
            return Err(OpenAiCompatibleTransportError::ResponseTooLarge);
        }
        let model = bounded_response_field(response.model, MAX_MODEL_BYTES)?;
        let request_id = bounded_response_field(response.id, MAX_MODEL_BYTES)?;
        let finish_reason = bounded_response_field(choice.finish_reason, 64)?;
        Ok(OpenAiCompatibleResponse {
            text: message,
            model,
            finish_reason,
            usage: response.usage.map(|usage| OpenAiCompatibleUsage {
                input_tokens: usage.prompt_tokens,
                output_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            }),
            request_id,
        })
    }
}

async fn read_bounded_body(
    response: reqwest::Response,
    max_response_bytes: usize,
) -> Result<Vec<u8>, OpenAiCompatibleTransportError> {
    if response
        .content_length()
        .is_some_and(|length| length > max_response_bytes as u64)
    {
        return Err(OpenAiCompatibleTransportError::ResponseTooLarge);
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
        let chunk = chunk.map_err(|_| OpenAiCompatibleTransportError::TransportUnavailable)?;
        if body.len().saturating_add(chunk.len()) > max_response_bytes {
            return Err(OpenAiCompatibleTransportError::ResponseTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn validate_base_url(
    value: String,
    policy: EndpointPolicy,
) -> Result<String, OpenAiCompatibleTransportError> {
    if value.len() > MAX_BASE_URL_BYTES || value.ends_with('/') {
        return Err(OpenAiCompatibleTransportError::InvalidConfiguration);
    }
    let url =
        Url::parse(&value).map_err(|_| OpenAiCompatibleTransportError::InvalidConfiguration)?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost"))
        && url.port().is_some_and(|port| port != 0);
    let allowed = match policy {
        EndpointPolicy::HttpsOnly => url.scheme() == "https",
        EndpointPolicy::LoopbackHttp => {
            url.scheme() == "https" || (url.scheme() == "http" && loopback)
        }
    };
    if !allowed
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(OpenAiCompatibleTransportError::InvalidConfiguration);
    }
    Ok(value)
}

fn map_status(
    status: reqwest::StatusCode,
    retry_after: Option<Duration>,
    body: &[u8],
) -> OpenAiCompatibleTransportError {
    match status.as_u16() {
        401 => OpenAiCompatibleTransportError::Unauthorized,
        402 => OpenAiCompatibleTransportError::PaymentRequired,
        403 => OpenAiCompatibleTransportError::Forbidden,
        408 => OpenAiCompatibleTransportError::RequestTimedOut,
        429 => OpenAiCompatibleTransportError::RateLimited {
            retry_after: retry_after.or_else(|| bounded_retry_after(body)),
        },
        500..=599 => OpenAiCompatibleTransportError::ProviderUnavailable,
        _ => OpenAiCompatibleTransportError::ProviderRejected,
    }
}

fn bounded_response_field(
    value: Option<String>,
    max_bytes: usize,
) -> Result<Option<String>, OpenAiCompatibleTransportError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() || value.len() > max_bytes {
        return Err(OpenAiCompatibleTransportError::InvalidResponse);
    }
    Ok(Some(value))
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    let seconds = value.parse::<u64>().ok()?;
    (seconds <= 3600).then(|| Duration::from_secs(seconds))
}

#[cfg(test)]
#[path = "openai_compatible_tests.rs"]
mod tests;

fn bounded_retry_after(body: &[u8]) -> Option<Duration> {
    let value = serde_json::from_slice::<RetryAfterBody>(body)
        .ok()
        .and_then(|body| body.retry_after)?;
    (value <= 3600).then(|| Duration::from_secs(value))
}

#[derive(Deserialize)]
struct RetryAfterBody {
    retry_after: Option<u64>,
}
