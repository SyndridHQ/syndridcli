use super::*;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::time::Duration;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::body_json;
use wiremock::matchers::header;
use wiremock::matchers::method;
use wiremock::matchers::path;

#[tokio::test]
async fn chat_completion_request_and_response_are_bounded_and_mapped() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .and(header("authorization", "Bearer bearer-sentinel"))
        .and(body_json(json!({
            "model": "openrouter/test-model",
            "messages": [
                {"role": "system", "content": "system instruction"},
                {"role": "user", "content": "user prompt"}
            ],
            "max_tokens": 128
        })))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(json!({
                    "id": "request-id",
                    "model": "openrouter/test-model",
                    "choices": [{
                        "message": {"content": "generated text"},
                        "finish_reason": "stop"
                    }],
                    "usage": {
                        "prompt_tokens": 3,
                        "completion_tokens": 2,
                        "total_tokens": 5
                    }
                })),
        )
        .mount(&server)
        .await;

    let transport = ReqwestOpenAiCompatibleTransport::new(
        server.uri(),
        "/chat/completions",
        EndpointPolicy::LoopbackHttp,
        Duration::from_secs(5),
        MAX_RESPONSE_BYTES,
    )
    .expect("loopback endpoint");
    let request = OpenAiCompatibleRequest::new(
        "openrouter/test-model",
        Some("system instruction".to_string()),
        "user prompt",
        128,
    )
    .expect("request");

    let response = transport
        .invoke("bearer-sentinel", request, CancellationToken::new())
        .await
        .expect("response");

    assert_eq!(response.text, "generated text");
    assert_eq!(response.model.as_deref(), Some("openrouter/test-model"));
    assert_eq!(response.request_id.as_deref(), Some("request-id"));
    assert_eq!(response.finish_reason.as_deref(), Some("stop"));
    assert_eq!(
        response.usage,
        Some(OpenAiCompatibleUsage {
            input_tokens: Some(3),
            output_tokens: Some(2),
            total_tokens: Some(5),
        })
    );
}

#[test]
fn request_and_response_debug_are_redacted() {
    let request = OpenAiCompatibleRequest::new(
        "model-sentinel",
        Some("system-sentinel".to_string()),
        "user-sentinel",
        128,
    )
    .expect("request");
    let response = OpenAiCompatibleResponse {
        text: "output-sentinel".to_string(),
        model: Some("model-sentinel".to_string()),
        finish_reason: Some("stop".to_string()),
        usage: None,
        request_id: Some("request-id".to_string()),
    };
    let debug = format!("{request:?} {response:?}");
    for sentinel in [
        "bearer-sentinel",
        "system-sentinel",
        "user-sentinel",
        "output-sentinel",
    ] {
        assert!(!debug.contains(sentinel), "debug leaked {sentinel}");
    }
    assert!(debug.contains("system_bytes"));
    assert!(debug.contains("generated_output_bytes"));
}

#[test]
fn provider_errors_do_not_include_raw_error_body() {
    let error = map_status(
        reqwest::StatusCode::BAD_REQUEST,
        None,
        br#"{"error":"raw-provider-error-body-sentinel"}"#,
    );
    assert!(!format!("{error:?}").contains("raw-provider-error-body-sentinel"));
    assert!(
        !error
            .to_string()
            .contains("raw-provider-error-body-sentinel")
    );
}

#[test]
fn openrouter_policy_rejects_plaintext_remote_endpoints() {
    let result = ReqwestOpenAiCompatibleTransport::new(
        "http://example.invalid",
        "/chat/completions",
        EndpointPolicy::HttpsOnly,
        Duration::from_secs(5),
        MAX_RESPONSE_BYTES,
    );
    assert!(matches!(
        result,
        Err(OpenAiCompatibleTransportError::InvalidConfiguration)
    ));
}

#[test]
fn loopback_policy_does_not_allow_arbitrary_plaintext() {
    let result = ReqwestOpenAiCompatibleTransport::new(
        "http://example.invalid",
        "/chat/completions",
        EndpointPolicy::LoopbackHttp,
        Duration::from_secs(5),
        MAX_RESPONSE_BYTES,
    );
    assert!(matches!(
        result,
        Err(OpenAiCompatibleTransportError::InvalidConfiguration)
    ));
}

#[test]
fn https_policy_accepts_valid_https_configuration() {
    let result = ReqwestOpenAiCompatibleTransport::new(
        "https://provider.example",
        "/chat/completions",
        EndpointPolicy::HttpsOnly,
        Duration::from_secs(5),
        MAX_RESPONSE_BYTES,
    );
    assert!(result.is_ok());
}

async fn transport_for_response(
    status: u16,
    content_type: Option<&str>,
    body: serde_json::Value,
    timeout: Duration,
    max_response_bytes: usize,
) -> Result<OpenAiCompatibleResponse, OpenAiCompatibleTransportError> {
    let server = MockServer::start().await;
    let mut response = ResponseTemplate::new(status).set_body_raw(
        serde_json::to_vec(&body).expect("test JSON"),
        content_type.unwrap_or("application/json"),
    );
    if let Some(content_type) = content_type {
        response = response.insert_header("content-type", content_type);
    }
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(response)
        .mount(&server)
        .await;
    let transport = ReqwestOpenAiCompatibleTransport::new(
        server.uri(),
        "/chat/completions",
        EndpointPolicy::LoopbackHttp,
        timeout,
        max_response_bytes,
    )
    .expect("loopback endpoint");
    transport
        .invoke(
            "bearer-sentinel",
            OpenAiCompatibleRequest::new("model", None, "user", 128).expect("request"),
            CancellationToken::new(),
        )
        .await
}

#[tokio::test]
async fn response_validation_and_status_mapping_are_bounded() {
    let cases = [
        (
            "invalid content type",
            200,
            Some("text/plain"),
            json!({}),
            OpenAiCompatibleTransportError::InvalidContentType,
        ),
        (
            "empty choices",
            200,
            Some("application/json"),
            json!({"choices": []}),
            OpenAiCompatibleTransportError::MissingOutput,
        ),
        (
            "empty output",
            200,
            Some("application/json"),
            json!({"choices": [{"message": {"content": "  "}}]}),
            OpenAiCompatibleTransportError::MissingOutput,
        ),
    ];
    for (name, status, content_type, body, expected) in cases {
        let result = transport_for_response(
            status,
            content_type,
            body,
            Duration::from_secs(5),
            MAX_RESPONSE_BYTES,
        )
        .await;
        assert_eq!(result, Err(expected), "case {name}");
    }
}

#[tokio::test]
async fn unknown_fields_are_ignored_and_missing_usage_stays_none() {
    let response = transport_for_response(
        200,
        Some("application/json"),
        json!({
            "unknown": "ignored",
            "choices": [{"message": {"content": "ok"}}]
        }),
        Duration::from_secs(5),
        MAX_RESPONSE_BYTES,
    )
    .await
    .expect("response");
    assert_eq!(response.text, "ok");
    assert_eq!(response.usage, None);
}

#[tokio::test]
async fn malformed_json_and_oversized_output_are_rejected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_raw(b"not-json".to_vec(), "application/json"),
        )
        .mount(&server)
        .await;
    let transport = ReqwestOpenAiCompatibleTransport::new(
        server.uri(),
        "/chat/completions",
        EndpointPolicy::LoopbackHttp,
        Duration::from_secs(5),
        MAX_RESPONSE_BYTES,
    )
    .expect("transport");
    let request = OpenAiCompatibleRequest::new("model", None, "user", 128).expect("request");
    assert_eq!(
        transport
            .invoke("bearer", request, CancellationToken::new())
            .await,
        Err(OpenAiCompatibleTransportError::InvalidResponse)
    );

    let result = transport_for_response(
        200,
        Some("application/json"),
        json!({"choices": [{"message": {"content": "output".repeat(MAX_MESSAGE_BYTES)}}]}),
        Duration::from_secs(5),
        MAX_RESPONSE_BYTES,
    )
    .await;
    assert_eq!(
        result,
        Err(OpenAiCompatibleTransportError::ResponseTooLarge)
    );
}

#[tokio::test]
async fn cancellation_and_timeout_are_distinct() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(100))
                .insert_header("content-type", "application/json")
                .set_body_json(json!({"choices": [{"message": {"content": "ok"}}]})),
        )
        .mount(&server)
        .await;
    let transport = ReqwestOpenAiCompatibleTransport::new(
        server.uri(),
        "/chat/completions",
        EndpointPolicy::LoopbackHttp,
        Duration::from_millis(10),
        MAX_RESPONSE_BYTES,
    )
    .expect("transport");
    let request = OpenAiCompatibleRequest::new("model", None, "user", 128).expect("request");
    assert_eq!(
        transport
            .invoke("bearer", request.clone(), CancellationToken::new())
            .await,
        Err(OpenAiCompatibleTransportError::RequestTimedOut)
    );
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert_eq!(
        transport.invoke("bearer", request, cancellation).await,
        Err(OpenAiCompatibleTransportError::Cancelled)
    );
}

#[tokio::test]
async fn response_limit_and_provider_statuses_are_mapped_without_body() {
    let result = transport_for_response(
        200,
        Some("application/json"),
        json!({"choices": [{"message": {"content": "x".repeat(MAX_RESPONSE_BYTES)}}]}),
        Duration::from_secs(5),
        32,
    )
    .await;
    assert_eq!(
        result,
        Err(OpenAiCompatibleTransportError::ResponseTooLarge)
    );

    for (status, expected) in [
        (401, OpenAiCompatibleTransportError::Unauthorized),
        (402, OpenAiCompatibleTransportError::PaymentRequired),
        (403, OpenAiCompatibleTransportError::Forbidden),
        (
            429,
            OpenAiCompatibleTransportError::RateLimited { retry_after: None },
        ),
        (500, OpenAiCompatibleTransportError::ProviderUnavailable),
        (400, OpenAiCompatibleTransportError::ProviderRejected),
        (302, OpenAiCompatibleTransportError::ProviderRejected),
    ] {
        let result = transport_for_response(
            status,
            Some("application/json"),
            json!({"error": "raw-provider-error-body-sentinel"}),
            Duration::from_secs(5),
            MAX_RESPONSE_BYTES,
        )
        .await;
        assert_eq!(result, Err(expected));
        assert!(!format!("{result:?}").contains("raw-provider-error-body-sentinel"));
        assert!(!format!("{result:?}").contains("bearer-sentinel"));
    }
}
