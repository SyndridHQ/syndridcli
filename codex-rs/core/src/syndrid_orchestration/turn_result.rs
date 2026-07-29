use super::BudgetExhaustionCategory;
use super::LiveOrchestrationOutcome;
use super::LiveOrchestrationTerminal;
use super::OrchestrationFailure;
use super::OrchestrationFailureKind;
use super::Retryability;

/// Maximum UTF-8 bytes permitted in a user-facing orchestration response.
pub const MAX_USER_FACING_RESPONSE_BYTES: usize = 32 * 1024;

/// Text that has been explicitly selected for presentation to the user.
///
/// This type is intentionally independent of rendering and transport types. Callers must pass
/// text produced by an approved final-synthesis boundary; orchestration evidence and raw role
/// output are not accepted as a substitute for that boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserFacingResponse(String);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UserFacingResponseError {
    pub actual_bytes: usize,
    pub max_bytes: usize,
}

impl UserFacingResponse {
    pub fn new(text: impl Into<String>) -> Result<Self, UserFacingResponseError> {
        let text = text.into();
        if text.len() > MAX_USER_FACING_RESPONSE_BYTES {
            return Err(UserFacingResponseError {
                actual_bytes: text.len(),
                max_bytes: MAX_USER_FACING_RESPONSE_BYTES,
            });
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn with_prefix(&self, prefix: &str) -> Result<Self, UserFacingResponseError> {
        let available = MAX_USER_FACING_RESPONSE_BYTES.saturating_sub(prefix.len());
        let mut end = self.0.len().min(available);
        while end > 0 && !self.0.is_char_boundary(end) {
            end -= 1;
        }
        let mut text = String::with_capacity(prefix.len() + end);
        text.push_str(prefix);
        text.push_str(&self.0[..end]);
        Self::new(text)
    }
}

impl AsRef<str> for UserFacingResponse {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Bounded, structured metadata retained with a translated turn result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationOperationalMetadata {
    pub run_id: String,
    pub provider_invocations: usize,
    pub tool_calls: usize,
    pub peak_concurrency: usize,
    pub synthesis_permitted: bool,
    pub cleanup_complete: bool,
}

/// Sanitized internal evidence retained for typed callers, but never forwarded as transcript text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrchestrationEvidence {
    pub terminal: LiveOrchestrationTerminal,
    pub failure: Option<OrchestrationFailure>,
    pub role_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestrationPartialCause {
    ResponseUnavailable,
    VerificationRejected,
    RepairExhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrchestrationCleanupFailure {
    Incomplete,
}

/// Typed, privacy-safe result of an orchestration turn.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OrchestrationTurnResult {
    Completed {
        response: UserFacingResponse,
        metadata: OrchestrationOperationalMetadata,
        evidence: OrchestrationEvidence,
    },
    Partial {
        response: UserFacingResponse,
        cause: OrchestrationPartialCause,
        metadata: OrchestrationOperationalMetadata,
        evidence: OrchestrationEvidence,
    },
    Failed {
        failure: OrchestrationFailure,
        user_message: UserFacingResponse,
        metadata: OrchestrationOperationalMetadata,
        evidence: OrchestrationEvidence,
    },
    Cancelled {
        user_message: UserFacingResponse,
        metadata: OrchestrationOperationalMetadata,
        evidence: OrchestrationEvidence,
    },
    TimedOut {
        user_message: UserFacingResponse,
        metadata: OrchestrationOperationalMetadata,
        evidence: OrchestrationEvidence,
    },
    BudgetExhausted {
        category: BudgetExhaustionCategory,
        user_message: UserFacingResponse,
        metadata: OrchestrationOperationalMetadata,
        evidence: OrchestrationEvidence,
    },
    CleanupIncomplete {
        cause: OrchestrationCleanupFailure,
        user_message: UserFacingResponse,
        metadata: OrchestrationOperationalMetadata,
        evidence: OrchestrationEvidence,
    },
}

/// Translates an authoritative coordinator outcome without changing terminal-cause ownership.
pub struct OrchestrationTurnResultBuilder;

impl OrchestrationTurnResultBuilder {
    pub fn build(
        outcome: &LiveOrchestrationOutcome,
        response_candidate: Option<UserFacingResponse>,
    ) -> OrchestrationTurnResult {
        let metadata = OrchestrationOperationalMetadata {
            run_id: outcome.run_id.clone(),
            provider_invocations: outcome.provider_invocations,
            tool_calls: outcome.tool_calls,
            peak_concurrency: outcome.peak_concurrency,
            synthesis_permitted: outcome.synthesis_permitted,
            cleanup_complete: outcome.observation.cleanup.complete.value == Some(true),
        };
        let evidence = OrchestrationEvidence {
            terminal: outcome.terminal,
            failure: outcome.failure,
            role_count: outcome.roles.len(),
        };

        if !metadata.cleanup_complete {
            return OrchestrationTurnResult::CleanupIncomplete {
                cause: OrchestrationCleanupFailure::Incomplete,
                user_message: response("The orchestration finished, but cleanup did not complete."),
                metadata,
                evidence,
            };
        }

        match outcome.terminal {
            LiveOrchestrationTerminal::Completed if outcome.synthesis_permitted => {
                OrchestrationTurnResult::Completed {
                    response: response_candidate.unwrap_or_else(|| {
                        response("The orchestration completed without an additional response.")
                    }),
                    metadata,
                    evidence,
                }
            }
            LiveOrchestrationTerminal::Completed => OrchestrationTurnResult::Partial {
                response: response_candidate.unwrap_or_else(|| {
                    response("The orchestration completed, but no final response was available.")
                }),
                cause: OrchestrationPartialCause::ResponseUnavailable,
                metadata,
                evidence,
            },
            LiveOrchestrationTerminal::Cancelled => OrchestrationTurnResult::Cancelled {
                user_message: response("The orchestration was cancelled."),
                metadata,
                evidence,
            },
            LiveOrchestrationTerminal::TimedOut => OrchestrationTurnResult::TimedOut {
                user_message: response("The orchestration timed out."),
                metadata,
                evidence,
            },
            LiveOrchestrationTerminal::BudgetExhausted => {
                let category = outcome
                    .budget_exhaustion_category
                    .unwrap_or(BudgetExhaustionCategory::TotalProviderInvocations);
                OrchestrationTurnResult::BudgetExhausted {
                    category,
                    user_message: response("The orchestration reached its execution budget."),
                    metadata,
                    evidence,
                }
            }
            LiveOrchestrationTerminal::Failed => {
                let failure = outcome.failure.unwrap_or(OrchestrationFailure {
                    kind: failure_kind(outcome),
                    retryability: Retryability::Unknown,
                    role: None,
                    tool: None,
                    terminal: outcome.terminal,
                });
                if failure.kind == OrchestrationFailureKind::RepairFailed {
                    return OrchestrationTurnResult::Partial {
                        response: response_candidate.unwrap_or_else(|| {
                            response("The work completed partially, but repair did not finish.")
                        }),
                        cause: OrchestrationPartialCause::RepairExhausted,
                        metadata,
                        evidence,
                    };
                }
                let cause = match failure.kind {
                    OrchestrationFailureKind::VerifierRejected => {
                        "The orchestration failed verification."
                    }
                    OrchestrationFailureKind::RepairFailed => {
                        "The orchestration could not repair the verification failure."
                    }
                    _ => "The orchestration failed before completion.",
                };
                OrchestrationTurnResult::Failed {
                    failure,
                    user_message: response(cause),
                    metadata,
                    evidence,
                }
            }
        }
    }
}

fn failure_kind(outcome: &LiveOrchestrationOutcome) -> OrchestrationFailureKind {
    match outcome.terminal_error {
        Some(super::LiveOrchestrationError::VerifierRejected) => {
            OrchestrationFailureKind::VerifierRejected
        }
        Some(super::LiveOrchestrationError::VerifierRuntimeFailure) => {
            OrchestrationFailureKind::VerifierProviderFailure
        }
        Some(super::LiveOrchestrationError::ExecutorBatchFailure) => {
            OrchestrationFailureKind::ExecutorBatchFailure
        }
        Some(super::LiveOrchestrationError::ExecutorJoinFailure) => {
            OrchestrationFailureKind::ExecutorJoinFailure
        }
        Some(super::LiveOrchestrationError::RepairFailed) => OrchestrationFailureKind::RepairFailed,
        Some(super::LiveOrchestrationError::Cancellation) => {
            OrchestrationFailureKind::UserCancelled
        }
        Some(super::LiveOrchestrationError::Timeout) => OrchestrationFailureKind::TotalTimedOut,
        Some(super::LiveOrchestrationError::BudgetExhaustionCategory(category)) => {
            OrchestrationFailureKind::BudgetExhausted(category)
        }
        _ => OrchestrationFailureKind::InternalCoordinatorFailure,
    }
}

fn response(text: &str) -> UserFacingResponse {
    UserFacingResponse::new(text)
        .unwrap_or_else(|_| unreachable!("static orchestration response must remain bounded"))
}

#[cfg(test)]
#[path = "turn_result_tests.rs"]
mod tests;
