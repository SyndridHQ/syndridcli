use super::CallbackServerError;
use super::OpenRouterCallbackServer;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;

async fn send_request(port: u16, request: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("callback listener connection");
    stream
        .write_all(request.as_bytes())
        .await
        .expect("callback request");
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .await
        .expect("callback response");
    response
}

#[tokio::test]
async fn binds_loopback_with_concrete_ephemeral_port() {
    let server = OpenRouterCallbackServer::bind().await.expect("bind");
    assert_ne!(server.port(), 0);
    assert_eq!(
        server.callback_uri(),
        format!("http://127.0.0.1:{}/callback", server.port())
    );
}

#[tokio::test]
async fn valid_callback_returns_o5d_completion_and_static_success_page() {
    let server = OpenRouterCallbackServer::bind().await.expect("bind");
    let port = server.port();
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(async move {
        server
            .wait_for_callback(&cancellation, Duration::from_secs(5))
            .await
    });
    let response = send_request(
        port,
        "GET /callback?state=test-state&code=test-code HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    )
    .await;
    let completion = task.await.expect("callback task").expect("completion");
    assert_eq!(completion.state, "test-state");
    assert_eq!(completion.code, "test-code");
    let response = String::from_utf8(response).expect("response text");
    assert!(response.contains("Content-Type: text/html; charset=utf-8"));
    assert!(response.contains("Connection: close"));
    assert!(response.contains("Authorization completed"));
    assert!(!response.contains("test-state"));
    assert!(!response.contains("test-code"));
}

#[tokio::test]
async fn malformed_requests_are_terminal_and_cancelled_wait_is_bounded() {
    let server = OpenRouterCallbackServer::bind().await.expect("bind");
    let port = server.port();
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(async move {
        server
            .wait_for_callback(&cancellation, Duration::from_secs(5))
            .await
    });
    let response = send_request(
        port,
        "POST /callback?state=test-state&code=test-code HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    )
    .await;
    assert_eq!(
        task.await.expect("callback task").unwrap_err(),
        CallbackServerError::MethodNotAllowed
    );
    assert!(
        String::from_utf8(response)
            .expect("response text")
            .contains("Authorization could not be completed")
    );

    let server = OpenRouterCallbackServer::bind().await.expect("bind");
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert_eq!(
        server
            .wait_for_callback(&cancellation, Duration::from_secs(5))
            .await
            .unwrap_err(),
        CallbackServerError::Cancelled
    );

    let server = OpenRouterCallbackServer::bind().await.expect("bind");
    assert_eq!(
        server
            .wait_for_callback(&CancellationToken::new(), Duration::ZERO)
            .await
            .unwrap_err(),
        CallbackServerError::Timeout
    );
}

#[tokio::test]
async fn wrong_path_and_duplicate_parameters_are_rejected() {
    let server = OpenRouterCallbackServer::bind().await.expect("bind");
    let port = server.port();
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(async move {
        server
            .wait_for_callback(&cancellation, Duration::from_secs(5))
            .await
    });
    let _response = send_request(
        port,
        "GET /wrong?state=test-state&code=test-code HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    )
    .await;
    assert_eq!(
        task.await.expect("callback task").unwrap_err(),
        CallbackServerError::InvalidRequest
    );
}

#[tokio::test]
async fn missing_code_and_state_are_rejected_with_static_failure_pages() {
    for target in ["/callback?state=state-only", "/callback?code=code-only"] {
        let server = OpenRouterCallbackServer::bind().await.expect("bind");
        let port = server.port();
        let cancellation = CancellationToken::new();
        let task = tokio::spawn(async move {
            server
                .wait_for_callback(&cancellation, Duration::from_secs(5))
                .await
        });
        let response = send_request(
            port,
            &format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"),
        )
        .await;
        assert_eq!(
            task.await.expect("callback task").unwrap_err(),
            CallbackServerError::CallbackRejected
        );
        let response = String::from_utf8(response).expect("response text");
        assert!(response.contains("Authorization could not be completed"));
        assert!(!response.contains("state-only"));
        assert!(!response.contains("code-only"));
    }
}

#[tokio::test]
async fn duplicate_parameters_and_oversized_requests_are_rejected_without_secrets() {
    let server = OpenRouterCallbackServer::bind().await.expect("bind");
    let port = server.port();
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(async move {
        server
            .wait_for_callback(&cancellation, Duration::from_secs(5))
            .await
    });
    let response = send_request(
        port,
        "GET /callback?state=one&state=two&code=code-sentinel HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
    )
    .await;
    assert_eq!(
        task.await.expect("callback task").unwrap_err(),
        CallbackServerError::InvalidRequest
    );
    let response = String::from_utf8(response).expect("response text");
    assert!(!response.contains("one"));
    assert!(!response.contains("two"));
    assert!(!response.contains("code-sentinel"));

    let server = OpenRouterCallbackServer::bind().await.expect("bind");
    let port = server.port();
    let cancellation = CancellationToken::new();
    let task = tokio::spawn(async move {
        server
            .wait_for_callback(&cancellation, Duration::from_secs(5))
            .await
    });
    let oversized = format!(
        "GET /callback HTTP/1.1\r\nX-Padding: {}\r\n\r\n",
        "x".repeat(9_000)
    );
    let _response = send_request(port, &oversized).await;
    assert_eq!(
        task.await.expect("callback task").unwrap_err(),
        CallbackServerError::RequestTooLarge
    );
}
