use super::credential_store::CredentialStore;
use super::credential_store::CredentialStoreError;
use super::provider_connection::AuthenticationMethod;
use super::provider_connection::ConnectionValidationResult;
use super::provider_connection::ConnectionValidationStatus;
use super::provider_connection::CredentialSecret;
use super::provider_connection::EndpointUrl;
use super::provider_connection::ProviderConnection;
use super::provider_connection::ProviderConnectionError;
use base64::Engine;
use codex_http_client::ClientRouteClass;
use codex_http_client::HttpClient;
use codex_http_client::HttpClientFactory;
use codex_http_client::OutboundProxyPolicy;
use rand::RngCore;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::fmt;
use std::time::Duration;
use url::Url;

const OPENROUTER_PROVIDER_ID: &str = "openrouter";
const OPENROUTER_AUTHORIZATION_ENDPOINT: &str = "https://openrouter.ai/auth";
const OPENROUTER_TOKEN_ENDPOINT: &str = "https://openrouter.ai/api/v1/auth/keys";
const MAX_AUTH_URL_BYTES: usize = 4096;
const MAX_CALLBACK_BYTES: usize = 2048;
const MAX_AUTH_VALUE_BYTES: usize = 128;
const MAX_TOKEN_RESPONSE_BYTES: usize = 16 * 1024;
const TOKEN_EXCHANGE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenRouterAuthError {
    InvalidConfiguration,
    InvalidAuthorizationEndpoint,
    InvalidTokenEndpoint,
    InvalidRedirectUri,
    InvalidCallback,
    InvalidState,
    StateMismatch,
    MissingAuthorizationCode,
    SessionAlreadyUsed,
    TransportUnavailable,
    Unauthorized,
    RateLimited,
    ProviderRejected,
    InvalidTokenResponse,
    MissingAccessToken,
    AccessTokenTooLong,
    CredentialStoreUnavailable,
    CredentialStoreRejected,
    CredentialNotFound,
    UnsupportedConnection,
}

impl fmt::Display for OpenRouterAuthError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidConfiguration => "OpenRouter authentication configuration is invalid",
            Self::InvalidAuthorizationEndpoint => "OpenRouter authorization endpoint is invalid",
            Self::InvalidTokenEndpoint => "OpenRouter token endpoint is invalid",
            Self::InvalidRedirectUri => "OpenRouter redirect URI is invalid",
            Self::InvalidCallback => "OpenRouter authorization callback is invalid",
            Self::InvalidState => "OAuth state is invalid",
            Self::StateMismatch => "OAuth state does not match the pending authorization",
            Self::MissingAuthorizationCode => "authorization code is missing",
            Self::SessionAlreadyUsed => "authorization session has already been used",
            Self::TransportUnavailable => "OpenRouter token transport is unavailable",
            Self::Unauthorized => "OpenRouter authorization was rejected",
            Self::RateLimited => "OpenRouter token exchange was rate limited",
            Self::ProviderRejected => "OpenRouter rejected the authorization",
            Self::InvalidTokenResponse => "OpenRouter token response is invalid",
            Self::MissingAccessToken => "OpenRouter token response did not contain a key",
            Self::AccessTokenTooLong => "OpenRouter access key exceeds its bounded length",
            Self::CredentialStoreUnavailable => "credential store is unavailable",
            Self::CredentialStoreRejected => "credential store rejected the credential",
            Self::CredentialNotFound => "OpenRouter credential was not found",
            Self::UnsupportedConnection => {
                "provider connection is not an OpenRouter PKCE connection"
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for OpenRouterAuthError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct OpenRouterAuthConfiguration {
    pub(super) authorization_endpoint: EndpointUrl,
    pub(super) token_endpoint: EndpointUrl,
    pub(super) redirect_uri: String,
}

impl OpenRouterAuthConfiguration {
    pub(super) fn default(redirect_uri: impl Into<String>) -> Result<Self, OpenRouterAuthError> {
        Self::new(
            OPENROUTER_AUTHORIZATION_ENDPOINT,
            OPENROUTER_TOKEN_ENDPOINT,
            redirect_uri,
        )
    }

    pub(super) fn new(
        authorization_endpoint: impl Into<String>,
        token_endpoint: impl Into<String>,
        redirect_uri: impl Into<String>,
    ) -> Result<Self, OpenRouterAuthError> {
        let authorization_endpoint = EndpointUrl::new(authorization_endpoint.into())
            .map_err(|_| OpenRouterAuthError::InvalidAuthorizationEndpoint)?;
        let token_endpoint = EndpointUrl::new(token_endpoint.into())
            .map_err(|_| OpenRouterAuthError::InvalidTokenEndpoint)?;
        if authorization_endpoint.as_str() != OPENROUTER_AUTHORIZATION_ENDPOINT
            || token_endpoint.as_str() != OPENROUTER_TOKEN_ENDPOINT
        {
            return Err(OpenRouterAuthError::InvalidConfiguration);
        }
        let redirect_uri = validate_redirect_uri(redirect_uri.into())?;
        Ok(Self {
            authorization_endpoint,
            token_endpoint,
            redirect_uri,
        })
    }
}

#[derive(Eq, PartialEq)]
pub(super) struct PkceVerifier(String);

impl PkceVerifier {
    fn generate() -> Self {
        let mut bytes = [0u8; 64];
        rand::rng().fill_bytes(&mut bytes);
        Self(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for PkceVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PkceVerifier(<redacted>)")
    }
}

impl fmt::Display for PkceVerifier {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PkceChallenge(String);

impl PkceChallenge {
    fn from_verifier(verifier: &PkceVerifier) -> Self {
        Self(
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .encode(Sha256::digest(verifier.as_str().as_bytes())),
        )
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct OAuthState(String);

impl OAuthState {
    fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
    }

    fn new(value: String) -> Result<Self, OpenRouterAuthError> {
        if value.trim().is_empty() || value.len() > MAX_AUTH_VALUE_BYTES {
            return Err(OpenRouterAuthError::InvalidState);
        }
        Ok(Self(value))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OAuthState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("OAuthState(<redacted>)")
    }
}

impl fmt::Display for OAuthState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct AuthorizationCode(String);

impl AuthorizationCode {
    fn new(value: String) -> Result<Self, OpenRouterAuthError> {
        if value.trim().is_empty() {
            return Err(OpenRouterAuthError::MissingAuthorizationCode);
        }
        if value.len() > MAX_AUTH_VALUE_BYTES {
            return Err(OpenRouterAuthError::InvalidTokenResponse);
        }
        Ok(Self(value))
    }
}

impl fmt::Debug for AuthorizationCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthorizationCode(<redacted>)")
    }
}

impl fmt::Display for AuthorizationCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("<redacted>")
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct OpenRouterAuthorizationRequest {
    pub(super) authorization_url: String,
    pub(super) redirect_uri: String,
    pub(super) state: OAuthState,
    pub(super) code_challenge: PkceChallenge,
}

impl fmt::Debug for OpenRouterAuthorizationRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenRouterAuthorizationRequest")
            .field("authorization_url", &"<redacted; use authorization_url()>")
            .field("redirect_uri", &"<redacted>")
            .field("state", &self.state)
            .field("code_challenge", &self.code_challenge)
            .finish()
    }
}

impl OpenRouterAuthorizationRequest {
    pub(super) fn authorization_url(&self) -> &str {
        &self.authorization_url
    }

    pub(super) fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(super) struct AuthorizationCompletion {
    pub(super) state: String,
    pub(super) code: String,
    callback_url: String,
}

impl fmt::Debug for AuthorizationCompletion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthorizationCompletion")
            .field("state", &"<redacted>")
            .field("code", &"<redacted>")
            .field("callback_url", &"<redacted>")
            .finish()
    }
}

impl AuthorizationCompletion {
    pub(super) fn from_callback_url(value: impl Into<String>) -> Result<Self, OpenRouterAuthError> {
        let value = value.into();
        if value.len() > MAX_CALLBACK_BYTES {
            return Err(OpenRouterAuthError::InvalidCallback);
        }
        let callback = Url::parse(&value).map_err(|_| OpenRouterAuthError::InvalidCallback)?;
        if callback.scheme() != "http"
            || !matches!(callback.host_str(), Some("127.0.0.1" | "localhost"))
            || callback.port().is_none()
            || callback.port() == Some(0)
            || callback.username() != ""
            || callback.password().is_some()
            || callback.fragment().is_some()
        {
            return Err(OpenRouterAuthError::InvalidCallback);
        }
        let state = callback
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.into_owned())
            .ok_or(OpenRouterAuthError::InvalidCallback)?;
        let code = callback
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value.into_owned())
            .ok_or(OpenRouterAuthError::MissingAuthorizationCode)?;
        AuthorizationCode::new(code.clone())?;
        Ok(Self {
            state,
            code,
            callback_url: value,
        })
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct OpenRouterAuthorizationSession {
    request: OpenRouterAuthorizationRequest,
    verifier: PkceVerifier,
}

impl OpenRouterAuthorizationSession {
    pub(super) fn request(&self) -> &OpenRouterAuthorizationRequest {
        &self.request
    }
}

#[derive(Debug)]
pub(super) struct OpenRouterTokenExchangeRequest {
    code: AuthorizationCode,
    verifier: PkceVerifier,
}

#[derive(Debug)]
pub(super) struct OpenRouterTokenResponse {
    pub(super) access_key: CredentialSecret,
}

pub(super) trait OpenRouterTokenTransport: Send + Sync {
    fn exchange(
        &self,
        request: OpenRouterTokenExchangeRequest,
    ) -> impl std::future::Future<Output = Result<OpenRouterTokenResponse, OpenRouterAuthError>> + Send;
}

#[derive(Clone, Debug)]
pub(super) struct OpenRouterHttpTransport {
    client: HttpClient,
    endpoint: String,
}

impl OpenRouterHttpTransport {
    // OpenRouter's dedicated PKCE guide requires POST /api/v1/auth/keys with code,
    // code_verifier, and code_challenge_method; this exchange sends no bearer token.
    pub(super) fn new(
        configuration: &OpenRouterAuthConfiguration,
    ) -> Result<Self, OpenRouterAuthError> {
        let client = HttpClientFactory::new(OutboundProxyPolicy::ReqwestDefault)
            .build_reqwest_client(
                reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()),
                configuration.token_endpoint.as_str(),
                ClientRouteClass::Auth,
            )
            .map(HttpClient::new)
            .map_err(|_| OpenRouterAuthError::TransportUnavailable)?;
        Ok(Self {
            client,
            endpoint: configuration.token_endpoint.to_string(),
        })
    }
}

#[derive(Serialize)]
struct OpenRouterTokenRequest<'a> {
    code: &'a str,
    code_verifier: &'a str,
    code_challenge_method: &'static str,
}

#[derive(Deserialize)]
struct OpenRouterTokenResponseBody {
    key: Option<String>,
}

impl OpenRouterTokenTransport for OpenRouterHttpTransport {
    async fn exchange(
        &self,
        request: OpenRouterTokenExchangeRequest,
    ) -> Result<OpenRouterTokenResponse, OpenRouterAuthError> {
        let response = self
            .client
            .post(&self.endpoint)
            .timeout(TOKEN_EXCHANGE_TIMEOUT)
            .json(&OpenRouterTokenRequest {
                code: &request.code.0,
                code_verifier: request.verifier.as_str(),
                code_challenge_method: "S256",
            })
            .send()
            .await
            .map_err(|_| OpenRouterAuthError::TransportUnavailable)?;
        let status = response.status();
        let content_type_is_json = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("application/json"));
        let body = read_bounded_body(response).await?;
        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => OpenRouterAuthError::Unauthorized,
                429 => OpenRouterAuthError::RateLimited,
                400..=499 => OpenRouterAuthError::ProviderRejected,
                _ => OpenRouterAuthError::TransportUnavailable,
            });
        }
        if !content_type_is_json {
            return Err(OpenRouterAuthError::InvalidTokenResponse);
        }
        let response: OpenRouterTokenResponseBody =
            serde_json::from_slice(&body).map_err(|_| OpenRouterAuthError::InvalidTokenResponse)?;
        let access_key = response
            .key
            .ok_or(OpenRouterAuthError::MissingAccessToken)?;
        let access_key = CredentialSecret::new(access_key).map_err(|error| match error {
            ProviderConnectionError::CredentialTooLong => OpenRouterAuthError::AccessTokenTooLong,
            _ => OpenRouterAuthError::MissingAccessToken,
        })?;
        Ok(OpenRouterTokenResponse { access_key })
    }
}

async fn read_bounded_body(response: reqwest::Response) -> Result<Vec<u8>, OpenRouterAuthError> {
    if response
        .content_length()
        .is_some_and(|length| length > MAX_TOKEN_RESPONSE_BYTES as u64)
    {
        return Err(OpenRouterAuthError::InvalidTokenResponse);
    }
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
        let chunk = chunk.map_err(|_| OpenRouterAuthError::TransportUnavailable)?;
        if body.len().saturating_add(chunk.len()) > MAX_TOKEN_RESPONSE_BYTES {
            return Err(OpenRouterAuthError::InvalidTokenResponse);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub(super) struct OpenRouterConnectionLifecycle<S, T> {
    configuration: OpenRouterAuthConfiguration,
    credential_store: S,
    transport: T,
    connection: ProviderConnection,
    pending: Option<OpenRouterAuthorizationSession>,
}

impl<S, T> OpenRouterConnectionLifecycle<S, T>
where
    S: CredentialStore,
    T: OpenRouterTokenTransport,
{
    pub(super) fn new(
        configuration: OpenRouterAuthConfiguration,
        credential_store: S,
        transport: T,
        connection: ProviderConnection,
    ) -> Result<Self, OpenRouterAuthError> {
        if connection.provider_id.as_str() != OPENROUTER_PROVIDER_ID
            || connection.authentication_method != AuthenticationMethod::OAuthPkce
        {
            return Err(OpenRouterAuthError::UnsupportedConnection);
        }
        Ok(Self {
            configuration,
            credential_store,
            transport,
            connection,
            pending: None,
        })
    }

    pub(super) fn begin_authorization(
        &mut self,
    ) -> Result<OpenRouterAuthorizationRequest, OpenRouterAuthError> {
        let state = OAuthState::generate();
        let verifier = PkceVerifier::generate();
        let challenge = PkceChallenge::from_verifier(&verifier);
        let redirect_uri = callback_uri(&self.configuration.redirect_uri, &state)?;
        let mut authorization_url = Url::parse(self.configuration.authorization_endpoint.as_str())
            .map_err(|_| OpenRouterAuthError::InvalidAuthorizationEndpoint)?;
        authorization_url
            .query_pairs_mut()
            .append_pair("callback_url", &redirect_uri)
            .append_pair("code_challenge", challenge.as_str())
            .append_pair("code_challenge_method", "S256");
        let authorization_url = authorization_url.to_string();
        if authorization_url.len() > MAX_AUTH_URL_BYTES {
            return Err(OpenRouterAuthError::InvalidAuthorizationEndpoint);
        }
        let request = OpenRouterAuthorizationRequest {
            authorization_url,
            redirect_uri,
            state,
            code_challenge: challenge,
        };
        let session = OpenRouterAuthorizationSession {
            request: request.clone(),
            verifier,
        };
        self.pending = Some(session);
        Ok(request)
    }

    pub(super) async fn complete_authorization(
        &mut self,
        completion: AuthorizationCompletion,
    ) -> Result<ProviderConnection, OpenRouterAuthError> {
        let session = self
            .pending
            .take()
            .ok_or(OpenRouterAuthError::SessionAlreadyUsed)?;
        if !callback_target_matches(session.request.redirect_uri(), &completion.callback_url) {
            return Err(OpenRouterAuthError::InvalidCallback);
        }
        let state = OAuthState::new(completion.state)?;
        if state != session.request.state {
            return Err(OpenRouterAuthError::StateMismatch);
        }
        let code = AuthorizationCode::new(completion.code)?;
        let token = self
            .transport
            .exchange(OpenRouterTokenExchangeRequest {
                code,
                verifier: session.verifier,
            })
            .await?;
        let reference = self
            .connection
            .credential_reference
            .clone()
            .ok_or(OpenRouterAuthError::UnsupportedConnection)?;
        self.credential_store
            .store(&reference, token.access_key)
            .map_err(map_store_error)?;
        self.connection.enabled = true;
        self.connection.validation = ConnectionValidationResult {
            status: ConnectionValidationStatus::Valid,
            error: None,
        };
        Ok(self.connection.clone())
    }

    pub(super) fn connection(&self) -> &ProviderConnection {
        &self.connection
    }

    pub(super) fn disconnect_local(&mut self) -> Result<(), OpenRouterAuthError> {
        let reference = self
            .connection
            .credential_reference
            .as_ref()
            .ok_or(OpenRouterAuthError::CredentialNotFound)?;
        self.credential_store
            .delete(reference)
            .map_err(map_store_error)?;
        self.connection.enabled = false;
        self.connection.validation = ConnectionValidationResult {
            status: ConnectionValidationStatus::Invalid,
            error: None,
        };
        Ok(())
    }
}

fn callback_uri(base: &str, state: &OAuthState) -> Result<String, OpenRouterAuthError> {
    let mut callback = Url::parse(base).map_err(|_| OpenRouterAuthError::InvalidRedirectUri)?;
    callback
        .query_pairs_mut()
        .append_pair("state", state.as_str());
    let callback = callback.to_string();
    if callback.len() > MAX_CALLBACK_BYTES {
        return Err(OpenRouterAuthError::InvalidRedirectUri);
    }
    Ok(callback)
}

fn validate_redirect_uri(value: String) -> Result<String, OpenRouterAuthError> {
    if value.len() > MAX_CALLBACK_BYTES {
        return Err(OpenRouterAuthError::InvalidRedirectUri);
    }
    let uri = Url::parse(&value).map_err(|_| OpenRouterAuthError::InvalidRedirectUri)?;
    if uri.scheme() != "http"
        || !matches!(uri.host_str(), Some("127.0.0.1" | "localhost"))
        || uri.port().is_none()
        || uri.port() == Some(0)
        || uri.username() != ""
        || uri.password().is_some()
        || uri.query().is_some()
        || uri.fragment().is_some()
    {
        return Err(OpenRouterAuthError::InvalidRedirectUri);
    }
    Ok(value)
}

fn callback_target_matches(expected: &str, actual: &str) -> bool {
    let Ok(expected) = Url::parse(expected) else {
        return false;
    };
    let Ok(actual) = Url::parse(actual) else {
        return false;
    };
    expected.scheme() == actual.scheme()
        && expected.host_str() == actual.host_str()
        && expected.port() == actual.port()
        && expected.path() == actual.path()
}

fn map_store_error(error: CredentialStoreError) -> OpenRouterAuthError {
    match error {
        CredentialStoreError::InvalidReference | CredentialStoreError::Rejected => {
            OpenRouterAuthError::CredentialStoreRejected
        }
        CredentialStoreError::NotFound => OpenRouterAuthError::CredentialNotFound,
        CredentialStoreError::Unavailable => OpenRouterAuthError::CredentialStoreUnavailable,
    }
}

#[cfg(test)]
#[path = "openrouter_auth_tests.rs"]
mod openrouter_auth_tests;
