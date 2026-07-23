use super::credential_store::CredentialStore;
use super::native_credential_store::NativeCredentialStore;
use super::openrouter_auth::OpenRouterAuthConfiguration;
use super::openrouter_auth::OpenRouterAuthError;
use super::openrouter_auth::OpenRouterConnectionLifecycle;
use super::openrouter_auth::OpenRouterHttpTransport;
use super::openrouter_auth::OpenRouterTokenTransport;
use super::openrouter_callback::CallbackServerError;
use super::openrouter_callback::DEFAULT_CALLBACK_TIMEOUT;
use super::openrouter_callback::OpenRouterCallbackServer;
use super::provider_connection::AuthenticationMethod;
use super::provider_connection::ConnectionLabel;
use super::provider_connection::CredentialReference;
use super::provider_connection::ProviderConnection;
use super::provider_connection::ProviderConnectionId;
use super::provider_connection::ProviderId;
use serde::Serialize;
use std::fmt;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const OPENROUTER_PROVIDER_ID: &str = "openrouter";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenRouterSetupRequest {
    pub connection_id: String,
    pub label: String,
    pub credential_reference: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserLaunchStatus {
    Opened,
    Failed,
}

#[derive(Clone, Eq, PartialEq)]
pub struct OpenRouterSetupStarted {
    authorization_url: String,
    callback_uri: String,
    browser_launch: BrowserLaunchStatus,
}

impl fmt::Debug for OpenRouterSetupStarted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OpenRouterSetupStarted")
            .field("authorization_url", &"<redacted; use authorization_url()>")
            .field("callback_uri", &"<redacted>")
            .field("browser_launch", &self.browser_launch)
            .finish()
    }
}

impl OpenRouterSetupStarted {
    pub fn authorization_url(&self) -> &str {
        &self.authorization_url
    }

    pub fn callback_uri(&self) -> &str {
        &self.callback_uri
    }

    pub fn browser_launch(&self) -> BrowserLaunchStatus {
        self.browser_launch
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenRouterSetupError {
    InvalidRequest,
    Callback(CallbackServerError),
    Authentication(OpenRouterAuthError),
}

impl fmt::Display for OpenRouterSetupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest => formatter.write_str("OpenRouter setup request is invalid"),
            Self::Callback(error) => error.fmt(formatter),
            Self::Authentication(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for OpenRouterSetupError {}

pub(super) trait BrowserLauncher: Send + Sync {
    fn open(&self, url: &str) -> Result<(), ()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SystemBrowserLauncher;

impl BrowserLauncher for SystemBrowserLauncher {
    fn open(&self, url: &str) -> Result<(), ()> {
        codex_login::open_browser(url)
    }
}

pub(super) struct OpenRouterSetupCoordinator<B> {
    browser_launcher: B,
    timeout: Duration,
}

/// Runs the user-facing OpenRouter authorization setup with the native credential store.
///
/// The callback receives only intentionally displayable setup metadata. Authentication secrets and
/// PKCE material remain owned by the setup coordinator and are never exposed to the caller.
pub async fn setup_openrouter(
    request: OpenRouterSetupRequest,
    cancellation: tokio_util::sync::CancellationToken,
    on_started: impl FnOnce(&OpenRouterSetupStarted),
) -> Result<(), OpenRouterSetupError> {
    let coordinator = OpenRouterSetupCoordinator::with_default_timeout(SystemBrowserLauncher);
    let session = coordinator.start(request).await?;
    on_started(session.started());
    session.finish(cancellation).await.map(|_| ())
}

impl<B: BrowserLauncher> OpenRouterSetupCoordinator<B> {
    pub(super) fn new(browser_launcher: B, timeout: Duration) -> Self {
        Self {
            browser_launcher,
            timeout,
        }
    }

    pub(super) fn with_default_timeout(browser_launcher: B) -> Self {
        Self::new(browser_launcher, DEFAULT_CALLBACK_TIMEOUT)
    }

    pub(super) async fn start(
        &self,
        request: OpenRouterSetupRequest,
    ) -> Result<
        OpenRouterSetupSession<NativeCredentialStore, OpenRouterHttpTransport>,
        OpenRouterSetupError,
    > {
        let callback_server = OpenRouterCallbackServer::bind()
            .await
            .map_err(OpenRouterSetupError::Callback)?;
        let configuration = OpenRouterAuthConfiguration::default(callback_server.callback_uri())
            .map_err(OpenRouterSetupError::Authentication)?;
        let transport = OpenRouterHttpTransport::new(&configuration)
            .map_err(OpenRouterSetupError::Authentication)?;
        self.start_with_dependencies(
            request,
            callback_server,
            configuration,
            NativeCredentialStore::new(),
            transport,
        )
        .await
    }

    pub(super) async fn start_with_dependencies<S, T>(
        &self,
        request: OpenRouterSetupRequest,
        callback_server: OpenRouterCallbackServer,
        configuration: OpenRouterAuthConfiguration,
        store: S,
        transport: T,
    ) -> Result<OpenRouterSetupSession<S, T>, OpenRouterSetupError>
    where
        S: CredentialStore,
        T: OpenRouterTokenTransport,
    {
        let connection = build_connection(request)?;
        let callback_uri = callback_server.callback_uri().to_string();
        let mut lifecycle =
            OpenRouterConnectionLifecycle::new(configuration, store, transport, connection)
                .map_err(OpenRouterSetupError::Authentication)?;
        let authorization_request = lifecycle
            .begin_authorization()
            .map_err(OpenRouterSetupError::Authentication)?;
        let browser_launch = if self
            .browser_launcher
            .open(authorization_request.authorization_url())
            .is_ok()
        {
            BrowserLaunchStatus::Opened
        } else {
            BrowserLaunchStatus::Failed
        };
        Ok(OpenRouterSetupSession {
            callback_server,
            lifecycle,
            started: OpenRouterSetupStarted {
                authorization_url: authorization_request.authorization_url().to_string(),
                callback_uri,
                browser_launch,
            },
            timeout: self.timeout,
        })
    }
}

pub(super) struct OpenRouterSetupSession<S, T> {
    callback_server: OpenRouterCallbackServer,
    lifecycle: OpenRouterConnectionLifecycle<S, T>,
    started: OpenRouterSetupStarted,
    timeout: Duration,
}

impl<S, T> OpenRouterSetupSession<S, T>
where
    S: CredentialStore,
    T: OpenRouterTokenTransport,
{
    pub(super) fn started(&self) -> &OpenRouterSetupStarted {
        &self.started
    }

    pub(super) async fn finish(
        self,
        cancellation: CancellationToken,
    ) -> Result<ProviderConnection, OpenRouterSetupError> {
        let OpenRouterSetupSession {
            callback_server,
            mut lifecycle,
            timeout,
            ..
        } = self;
        let completion = callback_server
            .wait_for_callback(&cancellation, timeout)
            .await
            .map_err(OpenRouterSetupError::Callback)?;
        lifecycle
            .complete_authorization(completion)
            .await
            .map_err(OpenRouterSetupError::Authentication)
    }
}

fn build_connection(
    request: OpenRouterSetupRequest,
) -> Result<ProviderConnection, OpenRouterSetupError> {
    let connection_id = ProviderConnectionId::new(request.connection_id)
        .map_err(|_| OpenRouterSetupError::InvalidRequest)?;
    let label =
        ConnectionLabel::new(request.label).map_err(|_| OpenRouterSetupError::InvalidRequest)?;
    let credential_reference = CredentialReference::new(request.credential_reference)
        .map_err(|_| OpenRouterSetupError::InvalidRequest)?;
    ProviderConnection::new(
        connection_id,
        ProviderId::new(OPENROUTER_PROVIDER_ID)
            .map_err(|_| OpenRouterSetupError::InvalidRequest)?,
        label,
        AuthenticationMethod::OAuthPkce,
        Some(credential_reference),
        None,
        false,
    )
    .map_err(|_| OpenRouterSetupError::InvalidRequest)
}

#[cfg(test)]
#[path = "openrouter_setup_tests.rs"]
mod openrouter_setup_tests;
