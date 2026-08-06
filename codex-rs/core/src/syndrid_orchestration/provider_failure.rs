//! Canonical, bounded classification of provider failures.
//!
//! This module intentionally accepts only structured, non-secret provider metadata. It does not
//! retain source errors, response bodies, headers, credentials, or requests, and it does not
//! decide whether a route should be retried or replaced.

use super::account_pools::AccountPoolProviderFamily;
use super::invocation::ProviderInvocationError;
use std::fmt;
use std::time::Duration;

/// The maximum cooldown hint accepted from a provider.
pub const MAX_PROVIDER_COOLDOWN: Duration = Duration::from_secs(24 * 60 * 60);

/// Canonical provider failure classes used by future cooldown and routing policy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProviderFailureClass {
    RateLimited,
    QuotaExhausted,
    Authentication,
    Authorization,
    InvalidRequest,
    ModelUnavailable,
    ContextLengthExceeded,
    ProviderUnavailable,
    Network,
    Timeout,
    Cancelled,
    Internal,
    Unknown,
}

/// Bounded provider-owned code retained as safe metadata.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProviderFailureCode(String);

impl ProviderFailureCode {
    const MAX_BYTES: usize = 64;

    pub fn new(value: impl Into<String>) -> Option<Self> {
        let value = value.into();
        if value.is_empty()
            || value.len() > Self::MAX_BYTES
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
        {
            return None;
        }
        Some(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Indicates how strongly the classification was supported.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProviderFailureEvidence {
    StructuredProviderCode,
    HttpStatus,
    TransportKind,
    Cancellation,
    SafeFallback,
    Unknown,
}

/// A bounded cooldown hint supplied by structured provider metadata.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderCooldownHint {
    RetryAfter(Duration),
}

impl ProviderCooldownHint {
    /// Parses delta-seconds without retaining the original header value.
    pub fn from_retry_after_seconds(seconds: u64) -> Option<Self> {
        let duration = Duration::from_secs(seconds);
        (duration > Duration::ZERO && duration <= MAX_PROVIDER_COOLDOWN)
            .then_some(Self::RetryAfter(duration))
    }

    /// Parses a validated delta-seconds Retry-After value.
    ///
    /// HTTP-date values are intentionally unsupported here because the canonical cooldown state
    /// uses monotonic time and this layer has no wall-clock authority.
    pub fn parse_retry_after(value: &str) -> Option<Self> {
        Self::from_retry_after_seconds(value.trim().parse().ok()?)
    }

    pub fn duration(self) -> Duration {
        match self {
            Self::RetryAfter(duration) => duration,
        }
    }
}

/// Structured transport evidence accepted by the canonical classifier.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProviderTransportKind {
    Network,
    Timeout,
    Internal,
}

/// Safe input to provider-failure classification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFailureInput {
    pub provider_family: AccountPoolProviderFamily,
    pub cancelled: bool,
    pub structured_code: Option<ProviderFailureCode>,
    pub transport: Option<ProviderTransportKind>,
    pub http_status: Option<u16>,
    pub retry_after_seconds: Option<u64>,
}

impl ProviderFailureInput {
    pub fn new(provider_family: AccountPoolProviderFamily) -> Self {
        Self {
            provider_family,
            cancelled: false,
            structured_code: None,
            transport: None,
            http_status: None,
            retry_after_seconds: None,
        }
    }

    pub fn with_structured_code(mut self, code: ProviderFailureCode) -> Self {
        self.structured_code = Some(code);
        self
    }

    pub fn with_http_status(mut self, status: u16) -> Self {
        self.http_status = Some(status);
        self
    }

    pub fn with_transport(mut self, transport: ProviderTransportKind) -> Self {
        self.transport = Some(transport);
        self
    }

    pub fn with_retry_after_seconds(mut self, seconds: u64) -> Self {
        self.retry_after_seconds = Some(seconds);
        self
    }

    pub fn cancelled(mut self) -> Self {
        self.cancelled = true;
        self
    }
}

/// Secret-free canonical provider failure information.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderFailureClassification {
    pub class: ProviderFailureClass,
    pub provider_family: AccountPoolProviderFamily,
    pub safe_code: Option<ProviderFailureCode>,
    pub http_status: Option<u16>,
    pub cooldown_hint: Option<ProviderCooldownHint>,
    pub evidence: ProviderFailureEvidence,
}

/// Classifies structured provider metadata with deterministic precedence.
pub fn classify_provider_failure(input: ProviderFailureInput) -> ProviderFailureClassification {
    let cooldown_hint = input
        .retry_after_seconds
        .and_then(ProviderCooldownHint::from_retry_after_seconds);
    let safe_code = input.structured_code.clone();
    let (class, evidence) = if input.cancelled {
        (
            ProviderFailureClass::Cancelled,
            ProviderFailureEvidence::Cancellation,
        )
    } else if let Some(code) = input.structured_code.as_ref() {
        (
            classify_code(code.as_str()),
            ProviderFailureEvidence::StructuredProviderCode,
        )
    } else if let Some(transport) = input.transport {
        (
            classify_transport(transport),
            ProviderFailureEvidence::TransportKind,
        )
    } else if let Some(status) = input.http_status {
        (
            classify_http_status(status),
            ProviderFailureEvidence::HttpStatus,
        )
    } else {
        (
            ProviderFailureClass::Unknown,
            ProviderFailureEvidence::Unknown,
        )
    };
    ProviderFailureClassification {
        class,
        provider_family: input.provider_family,
        safe_code,
        http_status: input.http_status,
        cooldown_hint,
        evidence,
    }
}

/// Maps the existing provider-neutral adapter error without changing routing behavior.
pub fn classify_provider_invocation_error(
    provider_family: AccountPoolProviderFamily,
    error: ProviderInvocationError,
) -> ProviderFailureClassification {
    let mut input = ProviderFailureInput::new(provider_family);
    let class = match error {
        ProviderInvocationError::Cancelled => {
            input.cancelled = true;
            ProviderFailureClass::Cancelled
        }
        ProviderInvocationError::RateLimited => ProviderFailureClass::RateLimited,
        ProviderInvocationError::PaymentRequired => ProviderFailureClass::QuotaExhausted,
        ProviderInvocationError::ReauthenticationRequired
        | ProviderInvocationError::MissingCredentialReference
        | ProviderInvocationError::CredentialNotFound
        | ProviderInvocationError::CredentialStoreUnavailable
        | ProviderInvocationError::CredentialStoreRejected
        | ProviderInvocationError::UnsupportedAuthenticationMethod
        | ProviderInvocationError::Unauthorized => ProviderFailureClass::Authentication,
        ProviderInvocationError::Forbidden => ProviderFailureClass::Authorization,
        ProviderInvocationError::InvalidModelId => ProviderFailureClass::ModelUnavailable,
        ProviderInvocationError::InputTooLarge => ProviderFailureClass::ContextLengthExceeded,
        ProviderInvocationError::InvalidRequest
        | ProviderInvocationError::InvalidConfiguration
        | ProviderInvocationError::OutputLimitInvalid => ProviderFailureClass::InvalidRequest,
        ProviderInvocationError::RequestTimedOut => ProviderFailureClass::Timeout,
        ProviderInvocationError::TransportUnavailable => ProviderFailureClass::Network,
        ProviderInvocationError::ProviderUnavailable
        | ProviderInvocationError::ProviderRejected => ProviderFailureClass::ProviderUnavailable,
        ProviderInvocationError::UnsupportedProvider
        | ProviderInvocationError::ConnectionDisabled
        | ProviderInvocationError::ConnectionUnvalidated
        | ProviderInvocationError::InvalidContentType
        | ProviderInvocationError::ResponseTooLarge
        | ProviderInvocationError::InvalidResponse
        | ProviderInvocationError::MissingOutput
        | ProviderInvocationError::OrchestrationConversionFailed
        | ProviderInvocationError::LiveCodexInvocationUnavailable
        | ProviderInvocationError::ScopedSessionConstructionFailed
        | ProviderInvocationError::StreamTerminated => ProviderFailureClass::Internal,
    };
    if input.cancelled {
        return classify_provider_failure(input);
    }
    ProviderFailureClassification {
        class,
        provider_family,
        safe_code: None,
        http_status: None,
        cooldown_hint: None,
        evidence: ProviderFailureEvidence::StructuredProviderCode,
    }
}

fn classify_code(code: &str) -> ProviderFailureClass {
    match code {
        "rate_limit_exceeded" => ProviderFailureClass::RateLimited,
        "insufficient_quota" => ProviderFailureClass::QuotaExhausted,
        "context_length_exceeded" => ProviderFailureClass::ContextLengthExceeded,
        _ => ProviderFailureClass::Unknown,
    }
}

fn classify_transport(transport: ProviderTransportKind) -> ProviderFailureClass {
    match transport {
        ProviderTransportKind::Network => ProviderFailureClass::Network,
        ProviderTransportKind::Timeout => ProviderFailureClass::Timeout,
        ProviderTransportKind::Internal => ProviderFailureClass::Internal,
    }
}

fn classify_http_status(status: u16) -> ProviderFailureClass {
    match status {
        401 => ProviderFailureClass::Authentication,
        402 => ProviderFailureClass::QuotaExhausted,
        403 => ProviderFailureClass::Authorization,
        408 => ProviderFailureClass::Timeout,
        413 | 422 => ProviderFailureClass::InvalidRequest,
        429 => ProviderFailureClass::RateLimited,
        500..=599 => ProviderFailureClass::ProviderUnavailable,
        400..=499 => ProviderFailureClass::InvalidRequest,
        _ => ProviderFailureClass::Unknown,
    }
}

impl fmt::Display for ProviderFailureClass {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::RateLimited => "rate_limited",
            Self::QuotaExhausted => "quota_exhausted",
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::InvalidRequest => "invalid_request",
            Self::ModelUnavailable => "model_unavailable",
            Self::ContextLengthExceeded => "context_length_exceeded",
            Self::ProviderUnavailable => "provider_unavailable",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::Internal => "internal",
            Self::Unknown => "unknown",
        })
    }
}

#[cfg(test)]
#[path = "provider_failure_tests.rs"]
mod tests;
