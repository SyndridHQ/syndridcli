use super::super::credential_store::CredentialStore;
use super::super::credential_store::CredentialStoreError;
use super::super::provider_connection::CredentialReference;
use super::super::provider_connection::CredentialSecret;
use super::BrowserLaunchStatus;
use super::BrowserLauncher;
use super::OpenRouterSetupCoordinator;
use super::OpenRouterSetupError;
use super::OpenRouterSetupRequest;
use super::OpenRouterSetupStarted;
use crate::syndrid_orchestration::openrouter_auth::OpenRouterAuthConfiguration;
use crate::syndrid_orchestration::openrouter_auth::OpenRouterTokenExchangeRequest;
use crate::syndrid_orchestration::openrouter_auth::OpenRouterTokenResponse;
use crate::syndrid_orchestration::openrouter_auth::OpenRouterTokenTransport;
use crate::syndrid_orchestration::openrouter_callback::OpenRouterCallbackServer;
use std::future::Future;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use url::Url;

const ACCESS_KEY: &str = "SYNDIRD_SECRET_MUST_NEVER_APPEAR_9f3d";

#[derive(Clone, Default)]
struct MockStore {
    values: Arc<Mutex<Vec<(String, String)>>>,
}

impl CredentialStore for MockStore {
    fn store(
        &self,
        reference: &CredentialReference,
        secret: CredentialSecret,
    ) -> Result<(), CredentialStoreError> {
        let mut values = self.values.lock().expect("store lock");
        values.retain(|(stored, _)| stored != reference.as_str());
        values.push((
            reference.as_str().to_string(),
            secret.expose_for_auth().to_string(),
        ));
        Ok(())
    }

    fn retrieve(
        &self,
        reference: &CredentialReference,
    ) -> Result<CredentialSecret, CredentialStoreError> {
        let value = self
            .values
            .lock()
            .expect("store lock")
            .iter()
            .find(|(stored, _)| stored == reference.as_str())
            .map(|(_, value)| value.clone())
            .ok_or(CredentialStoreError::NotFound)?;
        CredentialSecret::new(value).map_err(|_| CredentialStoreError::Rejected)
    }

    fn delete(&self, reference: &CredentialReference) -> Result<(), CredentialStoreError> {
        let mut values = self.values.lock().expect("store lock");
        let old_len = values.len();
        values.retain(|(stored, _)| stored != reference.as_str());
        (values.len() != old_len)
            .then_some(())
            .ok_or(CredentialStoreError::NotFound)
    }

    fn contains(&self, reference: &CredentialReference) -> Result<bool, CredentialStoreError> {
        Ok(self
            .values
            .lock()
            .expect("store lock")
            .iter()
            .any(|(stored, _)| stored == reference.as_str()))
    }
}

#[derive(Clone)]
struct MockTransport {
    calls: Arc<Mutex<usize>>,
}

impl OpenRouterTokenTransport for MockTransport {
    fn exchange(
        &self,
        _request: OpenRouterTokenExchangeRequest,
    ) -> impl Future<
        Output = Result<
            OpenRouterTokenResponse,
            super::super::openrouter_auth::OpenRouterAuthError,
        >,
    > + Send {
        let calls = Arc::clone(&self.calls);
        async move {
            *calls.lock().expect("calls lock") += 1;
            Ok(OpenRouterTokenResponse {
                access_key: CredentialSecret::new(ACCESS_KEY).expect("access key"),
            })
        }
    }
}

#[derive(Clone, Default)]
struct MockBrowser {
    urls: Arc<Mutex<Vec<String>>>,
    fail: bool,
}

impl BrowserLauncher for MockBrowser {
    fn open(&self, url: &str) -> Result<(), ()> {
        self.urls
            .lock()
            .expect("browser lock")
            .push(url.to_string());
        if self.fail { Err(()) } else { Ok(()) }
    }
}

fn setup_request() -> OpenRouterSetupRequest {
    OpenRouterSetupRequest {
        connection_id: "openrouter-connection".to_string(),
        label: "OpenRouter".to_string(),
        credential_reference: "openrouter-credential".to_string(),
    }
}

async fn start_test_setup(
    browser: MockBrowser,
) -> (
    super::OpenRouterSetupSession<MockStore, MockTransport>,
    MockBrowser,
    Arc<Mutex<usize>>,
) {
    let callback_server = OpenRouterCallbackServer::bind()
        .await
        .expect("callback bind");
    let configuration = OpenRouterAuthConfiguration::default(callback_server.callback_uri())
        .expect("configuration");
    let calls = Arc::new(Mutex::new(0));
    let coordinator = OpenRouterSetupCoordinator::new(browser.clone(), Duration::from_secs(5));
    let session = coordinator
        .start_with_dependencies(
            setup_request(),
            callback_server,
            configuration,
            MockStore::default(),
            MockTransport {
                calls: Arc::clone(&calls),
            },
        )
        .await
        .expect("setup start");
    (session, browser, calls)
}

fn callback_url(started: &OpenRouterSetupStarted, code: &str) -> String {
    let authorization = Url::parse(started.authorization_url()).expect("authorization URL");
    let callback = authorization
        .query_pairs()
        .find(|(key, _)| key == "callback_url")
        .map(|(_, value)| value.into_owned())
        .expect("callback URL");
    format!("{callback}&code={code}")
}

async fn send_callback(started: &OpenRouterSetupStarted, code: &str) {
    let callback = Url::parse(&callback_url(started, code)).expect("callback");
    let port = callback.port().expect("callback port");
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("callback connection");
    let target = callback.query().map_or_else(
        || callback.path().to_string(),
        |query| format!("{}?{query}", callback.path()),
    );
    let request = format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("callback request");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("callback response");
}

#[tokio::test]
async fn setup_allocates_concrete_port_and_opens_browser_once() {
    let browser = MockBrowser::default();
    let (session, browser, calls) = start_test_setup(browser).await;
    let started = session.started().clone();
    assert!(started.callback_uri().contains(":").then_some(()).is_some());
    assert!(!started.authorization_url().contains(":0/"));
    assert_eq!(started.browser_launch(), BrowserLaunchStatus::Opened);
    assert_eq!(browser.urls.lock().expect("browser lock").len(), 1);
    assert_eq!(*calls.lock().expect("calls lock"), 0);
    assert!(!format!("{started:?}").contains(started.authorization_url()));
}

#[tokio::test]
async fn browser_failure_is_nonfatal_and_copyable_url_remains_available() {
    let browser = MockBrowser {
        fail: true,
        ..MockBrowser::default()
    };
    let (session, browser, _) = start_test_setup(browser).await;
    assert_eq!(
        session.started().browser_launch(),
        BrowserLaunchStatus::Failed
    );
    assert!(!session.started().authorization_url().is_empty());
    assert_eq!(browser.urls.lock().expect("browser lock").len(), 1);
}

#[tokio::test]
async fn valid_callback_completes_o5d_and_cancellation_stops_waiting() {
    let (session, _, calls) = start_test_setup(MockBrowser::default()).await;
    let started = session.started().clone();
    let task = tokio::spawn(async move { session.finish(CancellationToken::new()).await });
    send_callback(&started, "authorization-code").await;
    let connection = task.await.expect("setup task").expect("setup result");
    assert!(connection.enabled);
    assert_eq!(*calls.lock().expect("calls lock"), 1);

    let (session, _, calls) = start_test_setup(MockBrowser::default()).await;
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert_eq!(
        session.finish(cancellation).await,
        Err(OpenRouterSetupError::Callback(
            super::super::openrouter_callback::CallbackServerError::Cancelled
        ))
    );
    assert_eq!(*calls.lock().expect("calls lock"), 0);
}

#[tokio::test]
async fn timeout_is_bounded_and_does_not_exchange() {
    let callback_server = OpenRouterCallbackServer::bind()
        .await
        .expect("callback bind");
    let configuration = OpenRouterAuthConfiguration::default(callback_server.callback_uri())
        .expect("configuration");
    let calls = Arc::new(Mutex::new(0));
    let coordinator = OpenRouterSetupCoordinator::new(MockBrowser::default(), Duration::ZERO);
    let session = coordinator
        .start_with_dependencies(
            setup_request(),
            callback_server,
            configuration,
            MockStore::default(),
            MockTransport {
                calls: Arc::clone(&calls),
            },
        )
        .await
        .expect("setup start");
    assert_eq!(
        session.finish(CancellationToken::new()).await,
        Err(OpenRouterSetupError::Callback(
            super::super::openrouter_callback::CallbackServerError::Timeout
        ))
    );
    assert_eq!(*calls.lock().expect("calls lock"), 0);
}
