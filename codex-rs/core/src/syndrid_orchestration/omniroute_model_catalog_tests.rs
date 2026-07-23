use super::*;
use crate::syndrid_orchestration::openai_compatible::EndpointPolicy;
use crate::syndrid_orchestration::openai_compatible::ReqwestOpenAiCompatibleTransport;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use wiremock::Mock;
use wiremock::MockServer;
use wiremock::ResponseTemplate;
use wiremock::matchers::method;
use wiremock::matchers::path;

async fn catalog(body: serde_json::Value) -> Result<Vec<String>, OmniRouteModelCatalogError> {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_json(body),
        )
        .mount(&server)
        .await;
    let transport = ReqwestOpenAiCompatibleTransport::new(
        server.uri(),
        "/v1/models",
        EndpointPolicy::LoopbackHttp,
        Duration::from_secs(5),
        256 * 1024,
    )
    .expect("transport");
    OmniRouteModelCatalogClient::new(transport)
        .list_with_bearer("key", CancellationToken::new())
        .await
}

#[tokio::test]
async fn catalog_is_sorted_deduplicated_and_ignores_unknown_fields() {
    let models = catalog(serde_json::json!({
        "object": "list",
        "unknown": "ignored",
        "data": [
            {"id": "z/model", "object": "model", "extra": 1},
            {"id": "a/model", "object": "model"},
            {"id": "z/model", "object": "model"}
        ]
    }))
    .await
    .expect("catalog");
    assert_eq!(models, vec!["a/model", "z/model"]);
}

#[tokio::test]
async fn catalog_rejects_blank_oversized_and_malformed_payloads() {
    assert_eq!(
        catalog(serde_json::json!({"data": [{"id": "  "}]})).await,
        Err(OmniRouteModelCatalogError::InvalidModelId)
    );
    assert_eq!(
        catalog(serde_json::json!({"data": [{"id": "x".repeat(257)}]})).await,
        Err(OmniRouteModelCatalogError::InvalidModelId)
    );
}

#[tokio::test]
async fn catalog_maps_http_failures_and_cancellation() {
    for (status, expected) in [
        (401, OmniRouteModelCatalogError::Unauthorized),
        (403, OmniRouteModelCatalogError::Forbidden),
        (429, OmniRouteModelCatalogError::RateLimited),
        (500, OmniRouteModelCatalogError::ProviderUnavailable),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("content-type", "application/json")
                    .set_body_raw(
                        br#"{"error":"raw-omniroute-error-body-sentinel"}"#.to_vec(),
                        "application/json",
                    ),
            )
            .mount(&server)
            .await;
        let transport = ReqwestOpenAiCompatibleTransport::new(
            server.uri(),
            "/v1/models",
            EndpointPolicy::LoopbackHttp,
            Duration::from_secs(5),
            256 * 1024,
        )
        .expect("transport");
        let error = OmniRouteModelCatalogClient::new(transport)
            .list_with_bearer("key", CancellationToken::new())
            .await
            .expect_err("HTTP error");
        assert_eq!(error, expected);
        assert!(
            !error
                .to_string()
                .contains("raw-omniroute-error-body-sentinel")
        );
    }
}

#[tokio::test]
async fn catalog_rejects_invalid_content_type_and_json() {
    for (content_type, body) in [
        ("text/plain", br#"{"data":[]}"#.as_slice()),
        ("application/json", br#"not-json"#.as_slice()),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", content_type)
                    .set_body_bytes(body),
            )
            .mount(&server)
            .await;
        let transport = ReqwestOpenAiCompatibleTransport::new(
            server.uri(),
            "/v1/models",
            EndpointPolicy::LoopbackHttp,
            Duration::from_secs(5),
            256 * 1024,
        )
        .expect("transport");
        let error = OmniRouteModelCatalogClient::new(transport)
            .list_with_bearer("key", CancellationToken::new())
            .await
            .expect_err("invalid catalog");
        assert!(matches!(
            error,
            OmniRouteModelCatalogError::InvalidContentType
                | OmniRouteModelCatalogError::InvalidResponse
        ));
    }
}

#[tokio::test]
async fn catalog_rejects_oversized_body_and_honors_cancellation() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_bytes(vec![b'x'; 300 * 1024]),
        )
        .mount(&server)
        .await;
    let transport = ReqwestOpenAiCompatibleTransport::new(
        server.uri(),
        "/v1/models",
        EndpointPolicy::LoopbackHttp,
        Duration::from_secs(5),
        256 * 1024,
    )
    .expect("transport");
    assert_eq!(
        OmniRouteModelCatalogClient::new(transport)
            .list_with_bearer("key", CancellationToken::new())
            .await,
        Err(OmniRouteModelCatalogError::ResponseTooLarge)
    );

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let server = MockServer::start().await;
    let transport = ReqwestOpenAiCompatibleTransport::new(
        server.uri(),
        "/v1/models",
        EndpointPolicy::LoopbackHttp,
        Duration::from_secs(5),
        256 * 1024,
    )
    .expect("transport");
    assert_eq!(
        OmniRouteModelCatalogClient::new(transport)
            .list_with_bearer("key", cancellation)
            .await,
        Err(OmniRouteModelCatalogError::Cancelled)
    );
}
