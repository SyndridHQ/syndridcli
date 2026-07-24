use super::CodexCredentialEnvelope;
use super::ProviderInvocationRequest;
use super::ScopedCodexSession;
use pretty_assertions::assert_eq;
use tokio_util::sync::CancellationToken;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

fn envelope() -> CodexCredentialEnvelope {
    CodexCredentialEnvelope::parse(
        &serde_json::json!({
            "schema_version": 1,
            "credential_kind": "chatgpt_oauth",
            "payload": {
                "id_token": "eyJhbGciOiJub25lIn0.eyJzdWIiOiIxIn0.sig",
                "access_token": "personal-access-token-sentinel",
                "refresh_token": "refresh-token-sentinel",
                "account_id": "personal-account-sentinel"
            }
        })
        .to_string(),
    )
    .expect("credential envelope")
}

#[tokio::test]
async fn scoped_session_uses_explicit_auth_and_maps_native_stream() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(
                    "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"scoped-account-ok\"}\n\n\
event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp-1\"}}\n\n",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;

    let session =
        ScopedCodexSession::new_with_base_url("codex-personal", &envelope(), &server.uri())
            .expect("scoped session");
    let result = session
        .invoke(
            ProviderInvocationRequest {
                provider: "codex".to_string(),
                model: "explicit-model".to_string(),
                system: None,
                user: "prompt-sentinel".to_string(),
                max_output_tokens: 64,
            },
            CancellationToken::new(),
        )
        .await
        .expect("scoped response");

    assert_eq!(result.text, "scoped-account-ok");
    let request = server.received_requests().await.expect("request")[0].clone();
    assert_eq!(
        request.headers.get("authorization").unwrap(),
        "Bearer personal-access-token-sentinel"
    );
    assert_eq!(
        request.headers.get("chatgpt-account-id").unwrap(),
        "personal-account-sentinel"
    );
    let body = String::from_utf8(request.body).expect("request body");
    assert!(body.contains("explicit-model"));
    assert!(body.contains("prompt-sentinel"));
    let debug = format!("{result:?}");
    assert!(!debug.contains("scoped-account-ok"));
    assert!(!debug.contains("prompt-sentinel"));
    let session_debug = format!("{session:?}");
    assert!(!session_debug.contains("personal-access-token-sentinel"));
    assert!(!session_debug.contains("personal-account-sentinel"));
}

#[tokio::test]
async fn scoped_session_cancellation_stops_before_result() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(
                    "event: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"output\"}\n\n",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;
    let session =
        ScopedCodexSession::new_with_base_url("codex-personal", &envelope(), &server.uri())
            .expect("scoped session");
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let error = session
        .invoke(
            ProviderInvocationRequest {
                provider: "codex".to_string(),
                model: "explicit-model".to_string(),
                system: None,
                user: "prompt".to_string(),
                max_output_tokens: 64,
            },
            cancellation,
        )
        .await
        .expect_err("cancelled invocation");
    assert_eq!(error, super::invocation::ProviderInvocationError::Cancelled);
}
