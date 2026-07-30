use super::LiveOrchestrationOutcome;
use super::LiveOrchestrationTerminal;
use super::UserFacingResponse;
use super::UserFacingResponseError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionFinalDeliverableInput {
    pub terminal: LiveOrchestrationTerminal,
    pub role_count: usize,
    pub provider_invocations: usize,
    pub tool_calls: usize,
}

impl ProductionFinalDeliverableInput {
    pub fn from_outcome(outcome: &LiveOrchestrationOutcome) -> Self {
        Self {
            terminal: outcome.terminal,
            role_count: outcome.roles.len(),
            provider_invocations: outcome.provider_invocations,
            tool_calls: outcome.tool_calls,
        }
    }
}

/// Produces a bounded user-facing summary from authoritative structured outcome data.
///
/// This producer deliberately does not inspect role text, provider payloads, tool output, or
/// observations. It is deterministic and is used until a separately approved synthesis contract
/// exists.
pub struct ProductionFinalDeliverableProducer;

impl ProductionFinalDeliverableProducer {
    pub fn produce(
        input: &ProductionFinalDeliverableInput,
    ) -> Result<UserFacingResponse, UserFacingResponseError> {
        let response = match input.terminal {
            LiveOrchestrationTerminal::Completed if input.role_count == 0 => {
                "No orchestration work was required.".to_string()
            }
            LiveOrchestrationTerminal::Completed => format!(
                "Orchestration completed with {role_count} role(s), {provider_invocations} provider invocation(s), and {tool_calls} approved tool call(s).",
                role_count = input.role_count,
                provider_invocations = input.provider_invocations,
                tool_calls = input.tool_calls,
            ),
            LiveOrchestrationTerminal::Failed => format!(
                "Orchestration stopped after {role_count} role(s), with {provider_invocations} provider invocation(s) and {tool_calls} approved tool call(s).",
                role_count = input.role_count,
                provider_invocations = input.provider_invocations,
                tool_calls = input.tool_calls,
            ),
            LiveOrchestrationTerminal::Cancelled => {
                "Orchestration was cancelled before completion.".to_string()
            }
            LiveOrchestrationTerminal::TimedOut => {
                "Orchestration timed out before completion.".to_string()
            }
            LiveOrchestrationTerminal::BudgetExhausted => {
                "Orchestration reached its execution budget before completion.".to_string()
            }
        };
        UserFacingResponse::new(response)
    }

    pub fn from_outcome(
        outcome: &LiveOrchestrationOutcome,
    ) -> Result<UserFacingResponse, UserFacingResponseError> {
        Self::produce(&ProductionFinalDeliverableInput::from_outcome(outcome))
    }
}
