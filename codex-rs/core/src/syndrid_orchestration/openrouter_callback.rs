use super::openrouter_auth::AuthorizationCompletion;
use serde::Serialize;
use std::fmt;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use url::Url;

const CALLBACK_PATH: &str = "/callback";
const MAX_REQUEST_BYTES: usize = 8 * 1024;
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const DEFAULT_CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);
const SUCCESS_HTML: &[u8] =
    b"<!doctype html><html><body>Authorization completed. You can return to Syndrid.</body></html>";
const FAILURE_HTML: &[u8] = b"<!doctype html><html><body>Authorization could not be completed. Return to Syndrid for details.</body></html>";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackServerError {
    BindFailed,
    AddressUnavailable,
    Timeout,
    Cancelled,
    InvalidRequest,
    MethodNotAllowed,
    RequestTooLarge,
    CallbackRejected,
}

impl fmt::Display for CallbackServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BindFailed => "OpenRouter callback listener could not bind",
            Self::AddressUnavailable => "OpenRouter callback listener address unavailable",
            Self::Timeout => "OpenRouter authorization callback timed out",
            Self::Cancelled => "OpenRouter authorization was cancelled",
            Self::InvalidRequest => "OpenRouter callback request was invalid",
            Self::MethodNotAllowed => "OpenRouter callback method was not allowed",
            Self::RequestTooLarge => "OpenRouter callback request was too large",
            Self::CallbackRejected => "OpenRouter callback was rejected",
        })
    }
}

impl std::error::Error for CallbackServerError {}

#[derive(Debug)]
pub(super) struct OpenRouterCallbackServer {
    listener: TcpListener,
    callback_uri: String,
}

impl OpenRouterCallbackServer {
    pub(super) async fn bind() -> Result<Self, CallbackServerError> {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .map_err(|_| CallbackServerError::BindFailed)?;
        let port = listener
            .local_addr()
            .map_err(|_| CallbackServerError::AddressUnavailable)?
            .port();
        if port == 0 {
            return Err(CallbackServerError::AddressUnavailable);
        }
        Ok(Self {
            listener,
            callback_uri: format!("http://127.0.0.1:{port}{CALLBACK_PATH}"),
        })
    }

    pub(super) fn callback_uri(&self) -> &str {
        &self.callback_uri
    }

    pub(super) fn port(&self) -> u16 {
        self.listener
            .local_addr()
            .expect("callback listener address remains available")
            .port()
    }

    pub(super) async fn wait_for_callback(
        self,
        cancellation: &CancellationToken,
        timeout: Duration,
    ) -> Result<AuthorizationCompletion, CallbackServerError> {
        match tokio::time::timeout(timeout, self.wait_until_terminal(cancellation)).await {
            Ok(result) => result,
            Err(_) => Err(CallbackServerError::Timeout),
        }
    }

    async fn wait_until_terminal(
        self,
        cancellation: &CancellationToken,
    ) -> Result<AuthorizationCompletion, CallbackServerError> {
        let (mut stream, _) = tokio::select! {
            _ = cancellation.cancelled() => return Err(CallbackServerError::Cancelled),
            accepted = self.listener.accept() => {
                accepted.map_err(|_| CallbackServerError::InvalidRequest)?
            }
        };
        let result =
            match tokio::time::timeout(REQUEST_READ_TIMEOUT, read_request(&mut stream)).await {
                Ok(Ok(request)) => parse_callback_request(&request, self.callback_uri()),
                Ok(Err(error)) => Err(error),
                Err(_) => Err(CallbackServerError::InvalidRequest),
            };
        match result {
            Ok(_) => write_response(&mut stream, 200, SUCCESS_HTML).await,
            Err(_) => write_response(&mut stream, 400, FAILURE_HTML).await,
        }
        .map_err(|_| CallbackServerError::InvalidRequest)?;
        result
    }
}

async fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, CallbackServerError> {
    let mut request = Vec::with_capacity(1024);
    loop {
        let mut chunk = [0u8; 1024];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|_| CallbackServerError::InvalidRequest)?;
        if read == 0 {
            return Err(CallbackServerError::InvalidRequest);
        }
        request.extend_from_slice(&chunk[..read]);
        if request.len() > MAX_REQUEST_BYTES {
            return Err(CallbackServerError::RequestTooLarge);
        }
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            return Ok(request);
        }
    }
}

fn parse_callback_request(
    request: &[u8],
    callback_uri: &str,
) -> Result<AuthorizationCompletion, CallbackServerError> {
    let request = std::str::from_utf8(request).map_err(|_| CallbackServerError::InvalidRequest)?;
    let request_line = request
        .split_once("\r\n")
        .map(|(line, _)| line)
        .ok_or(CallbackServerError::InvalidRequest)?;
    let mut parts = request_line.split_ascii_whitespace();
    let method = parts.next().ok_or(CallbackServerError::InvalidRequest)?;
    let target = parts.next().ok_or(CallbackServerError::InvalidRequest)?;
    let version = parts.next().ok_or(CallbackServerError::InvalidRequest)?;
    if parts.next().is_some() || !matches!(version, "HTTP/1.0" | "HTTP/1.1") {
        return Err(CallbackServerError::InvalidRequest);
    }
    if method != "GET" {
        return Err(CallbackServerError::MethodNotAllowed);
    }
    if !target.starts_with('/') || target.contains('#') {
        return Err(CallbackServerError::InvalidRequest);
    }
    let expected = Url::parse(callback_uri).map_err(|_| CallbackServerError::InvalidRequest)?;
    let port = expected.port().ok_or(CallbackServerError::InvalidRequest)?;
    let callback = Url::parse(&format!("http://127.0.0.1:{port}{target}"))
        .map_err(|_| CallbackServerError::InvalidRequest)?;
    if callback.path() != expected.path() {
        return Err(CallbackServerError::InvalidRequest);
    }
    let state_count = callback
        .query_pairs()
        .filter(|(key, _)| key == "state")
        .count();
    let code_count = callback
        .query_pairs()
        .filter(|(key, _)| key == "code")
        .count();
    if state_count > 1 || code_count > 1 {
        return Err(CallbackServerError::InvalidRequest);
    }
    AuthorizationCompletion::from_callback_url(callback.to_string())
        .map_err(|_| CallbackServerError::CallbackRejected)
}

async fn write_response(stream: &mut TcpStream, status: u16, body: &[u8]) -> std::io::Result<()> {
    let response = format!(
        "HTTP/1.1 {status} {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        if status == 200 { "OK" } else { "Bad Request" },
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.shutdown().await
}

#[cfg(test)]
#[path = "openrouter_callback_tests.rs"]
mod openrouter_callback_tests;
