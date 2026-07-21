use serde::Deserialize;
use serde::Serialize;

/// Selects orchestration policy; it does not execute that policy.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrchestrationMode {
    /// Preserve the normal single-agent Codex path.
    Single,
    /// Let the user define the workflow.
    Manual,
    /// Propose a workflow for user confirmation.
    Recommended,
    /// Choose a bounded workflow under a usage ceiling.
    Automatic,
    /// Allocate usage across a quota period conservatively.
    Adaptive,
}
