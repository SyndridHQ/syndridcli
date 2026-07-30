use super::LiveOrchestrationTerminal;
use super::final_deliverable::ProductionFinalDeliverableInput;
use super::final_deliverable::ProductionFinalDeliverableProducer;
use pretty_assertions::assert_eq;

#[test]
fn deterministic_deliverable_uses_only_bounded_structured_counts() {
    let input = ProductionFinalDeliverableInput {
        terminal: LiveOrchestrationTerminal::Completed,
        role_count: 1,
        provider_invocations: 1,
        tool_calls: 0,
    };
    let response = ProductionFinalDeliverableProducer::produce(&input).expect("response");
    assert_eq!(
        response.as_str(),
        "Orchestration completed with 1 role(s), 1 provider invocation(s), and 0 approved tool call(s)."
    );
}

#[test]
fn deterministic_deliverable_preserves_terminal_classification() {
    let input = ProductionFinalDeliverableInput {
        terminal: LiveOrchestrationTerminal::Cancelled,
        role_count: 1,
        provider_invocations: 1,
        tool_calls: 0,
    };
    let response = ProductionFinalDeliverableProducer::produce(&input).expect("response");
    assert_eq!(
        response.as_str(),
        "Orchestration was cancelled before completion."
    );
}
