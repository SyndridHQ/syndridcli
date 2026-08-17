use super::AccountPoolProviderFamily;
use super::ProviderCooldownHint;
use super::ProviderCooldownRecordingDecision;
use super::ProviderFailureClass;
use super::ProviderFailureCode;
use super::ProviderFailureEvidence;
use super::ProviderFailureInput;
use super::ProviderInvocationError;
use super::ProviderTransportKind;
use super::classify_provider_failure;
use super::classify_provider_invocation_error;
use super::cooldown_recording_decision;
use std::time::Duration;

fn input() -> ProviderFailureInput {
    ProviderFailureInput::new(AccountPoolProviderFamily::NativeCodex)
}

#[test]
fn structured_code_precedes_transport_and_http_status() {
    let classification = classify_provider_failure(
        input()
            .with_structured_code(ProviderFailureCode::new("insufficient_quota").unwrap())
            .with_transport(ProviderTransportKind::Network)
            .with_http_status(403),
    );
    assert_eq!(classification.class, ProviderFailureClass::QuotaExhausted);
    assert_eq!(
        classification.evidence,
        ProviderFailureEvidence::StructuredProviderCode
    );
    assert_eq!(classification.http_status, Some(403));
}

#[test]
fn cancellation_precedes_all_other_evidence() {
    let classification = classify_provider_failure(
        input()
            .cancelled()
            .with_structured_code(ProviderFailureCode::new("insufficient_quota").unwrap())
            .with_http_status(429),
    );
    assert_eq!(classification.class, ProviderFailureClass::Cancelled);
    assert_eq!(
        classification.evidence,
        ProviderFailureEvidence::Cancellation
    );
}

#[test]
fn transport_precedes_http_fallback() {
    let classification = classify_provider_failure(
        input()
            .with_transport(ProviderTransportKind::Timeout)
            .with_http_status(503),
    );
    assert_eq!(classification.class, ProviderFailureClass::Timeout);
    assert_eq!(
        classification.evidence,
        ProviderFailureEvidence::TransportKind
    );
}

#[test]
fn http_fallback_is_conservative_and_supports_retry_after() {
    let classification =
        classify_provider_failure(input().with_http_status(429).with_retry_after_seconds(30));
    assert_eq!(classification.class, ProviderFailureClass::RateLimited);
    assert_eq!(classification.evidence, ProviderFailureEvidence::HttpStatus);
    assert_eq!(
        classification
            .cooldown_hint
            .map(ProviderCooldownHint::duration),
        Some(std::time::Duration::from_secs(30))
    );
    assert_eq!(
        classify_provider_failure(input().with_http_status(403)).class,
        ProviderFailureClass::Authorization
    );
    assert_eq!(
        classify_provider_failure(input().with_http_status(413)).class,
        ProviderFailureClass::InvalidRequest
    );
    assert_eq!(
        classify_provider_failure(input().with_http_status(503)).class,
        ProviderFailureClass::ProviderUnavailable
    );
    assert_eq!(
        classify_provider_failure(input().with_http_status(399)).class,
        ProviderFailureClass::Unknown
    );
}

#[test]
fn invalid_retry_after_is_not_retained() {
    for seconds in [0, 24 * 60 * 60 + 1] {
        let classification = classify_provider_failure(
            input()
                .with_http_status(429)
                .with_retry_after_seconds(seconds),
        );
        assert_eq!(classification.cooldown_hint, None);
    }
    assert!(ProviderFailureCode::new("raw header value").is_none());
    assert_eq!(
        ProviderCooldownHint::parse_retry_after("30")
            .unwrap()
            .duration(),
        std::time::Duration::from_secs(30)
    );
    assert_eq!(
        ProviderCooldownHint::parse_retry_after("not-a-duration"),
        None
    );
    assert_eq!(ProviderCooldownHint::parse_retry_after("0"), None);
}

#[test]
fn unknown_structured_code_does_not_fall_back_to_http() {
    let classification = classify_provider_failure(
        input()
            .with_structured_code(
                ProviderFailureCode::new("provider_specific_future_code").unwrap(),
            )
            .with_http_status(503),
    );
    assert_eq!(classification.class, ProviderFailureClass::Unknown);
    assert_eq!(
        classification.evidence,
        ProviderFailureEvidence::StructuredProviderCode
    );
}

#[test]
fn structured_codes_cover_the_canonical_failure_taxonomy() {
    let cases = [
        ("rate_limit_exceeded", ProviderFailureClass::RateLimited),
        ("insufficient_quota", ProviderFailureClass::QuotaExhausted),
        (
            "context_length_exceeded",
            ProviderFailureClass::ContextLengthExceeded,
        ),
    ];
    for (code, expected) in cases {
        assert_eq!(
            classify_provider_failure(
                input().with_structured_code(ProviderFailureCode::new(code).unwrap())
            )
            .class,
            expected
        );
    }
    assert_eq!(
        classify_provider_failure(
            input().with_structured_code(ProviderFailureCode::new("future_code").unwrap())
        )
        .class,
        ProviderFailureClass::Unknown
    );
}

#[test]
fn native_and_omniroute_adapter_errors_use_the_same_canonical_mapping() {
    assert_eq!(
        classify_provider_invocation_error(
            AccountPoolProviderFamily::NativeCodex,
            ProviderInvocationError::RateLimited,
        )
        .class,
        ProviderFailureClass::RateLimited
    );
    assert_eq!(
        classify_provider_invocation_error(
            AccountPoolProviderFamily::OmniRoute,
            ProviderInvocationError::InputTooLarge,
        )
        .class,
        ProviderFailureClass::ContextLengthExceeded
    );
    assert_eq!(
        classify_provider_invocation_error(
            AccountPoolProviderFamily::OmniRoute,
            ProviderInvocationError::Unauthorized,
        )
        .class,
        ProviderFailureClass::Authentication
    );
    assert_eq!(
        classify_provider_invocation_error(
            AccountPoolProviderFamily::OmniRoute,
            ProviderInvocationError::RateLimitedWithRetryAfter(Some(Duration::from_secs(60))),
        )
        .cooldown_hint
        .map(ProviderCooldownHint::duration),
        Some(Duration::from_secs(60))
    );
}

#[test]
fn cooldown_recording_requires_an_attempt_eligible_class_and_hint() {
    let classification =
        classify_provider_failure(input().with_http_status(429).with_retry_after_seconds(60));
    assert_eq!(
        cooldown_recording_decision(&classification, true),
        ProviderCooldownRecordingDecision::Record {
            duration: Duration::from_secs(60),
            failure_class: ProviderFailureClass::RateLimited,
            safe_code: None,
        }
    );
    assert_eq!(
        cooldown_recording_decision(&classification, false),
        ProviderCooldownRecordingDecision::DoNotRecord
    );
    let no_hint = classify_provider_failure(input().with_http_status(503));
    assert_eq!(
        cooldown_recording_decision(&no_hint, true),
        ProviderCooldownRecordingDecision::DoNotRecord
    );
    let cancelled = classify_provider_failure(input().cancelled().with_retry_after_seconds(60));
    assert_eq!(
        cooldown_recording_decision(&cancelled, true),
        ProviderCooldownRecordingDecision::DoNotRecord
    );
}

#[test]
fn adapter_error_mapping_covers_remaining_canonical_classes() {
    let cases = [
        (
            ProviderInvocationError::PaymentRequired,
            ProviderFailureClass::QuotaExhausted,
        ),
        (
            ProviderInvocationError::ReauthenticationRequired,
            ProviderFailureClass::Authentication,
        ),
        (
            ProviderInvocationError::Forbidden,
            ProviderFailureClass::Authorization,
        ),
        (
            ProviderInvocationError::InvalidRequest,
            ProviderFailureClass::InvalidRequest,
        ),
        (
            ProviderInvocationError::InvalidModelId,
            ProviderFailureClass::ModelUnavailable,
        ),
        (
            ProviderInvocationError::InputTooLarge,
            ProviderFailureClass::ContextLengthExceeded,
        ),
        (
            ProviderInvocationError::ProviderUnavailable,
            ProviderFailureClass::ProviderUnavailable,
        ),
        (
            ProviderInvocationError::TransportUnavailable,
            ProviderFailureClass::Network,
        ),
        (
            ProviderInvocationError::RequestTimedOut,
            ProviderFailureClass::Timeout,
        ),
        (
            ProviderInvocationError::Cancelled,
            ProviderFailureClass::Cancelled,
        ),
        (
            ProviderInvocationError::OrchestrationConversionFailed,
            ProviderFailureClass::Internal,
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(
            classify_provider_invocation_error(AccountPoolProviderFamily::NativeCodex, error).class,
            expected
        );
    }
    assert_eq!(
        classify_provider_failure(input()).class,
        ProviderFailureClass::Unknown
    );
}
