//! Neutral, in-process contracts for a future production orchestration turn.
//!
//! The app-server owns these contracts because it owns turn admission. The
//! concrete runner remains supplied by a trusted in-process composition root;
//! these types intentionally contain no provider, TUI, or protocol authority.

use crate::in_process::InProcessServerEvent;
use codex_core::ProductionCancellationReason;
use codex_core::ProductionOrchestrationCancellationHandle;
use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;

pub(crate) const MAX_PRODUCTION_TURN_ID_BYTES: usize = 256;
pub(crate) const MAX_PRODUCTION_OBJECTIVE_BYTES: usize = 32 * 1024;
pub(crate) const MAX_PRODUCTION_CONTEXT_BYTES: usize = 32 * 1024;

/// Bounded, immutable data captured by app-server at production-turn admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductionTurnAdmissionInput {
    turn_id: String,
    thread_id: String,
    objective: String,
    workspace_root: PathBuf,
}

impl ProductionTurnAdmissionInput {
    pub fn new(
        turn_id: impl Into<String>,
        thread_id: impl Into<String>,
        objective: impl Into<String>,
        workspace_root: PathBuf,
    ) -> Result<Self, ProductionTurnPreparationError> {
        let turn_id = turn_id.into();
        let thread_id = thread_id.into();
        let objective = objective.into();
        validate_text("turn_id", &turn_id, MAX_PRODUCTION_TURN_ID_BYTES, false)?;
        validate_text("thread_id", &thread_id, MAX_PRODUCTION_TURN_ID_BYTES, false)?;
        validate_text(
            "objective",
            &objective,
            MAX_PRODUCTION_OBJECTIVE_BYTES,
            true,
        )?;
        if !workspace_root.is_absolute() {
            return Err(ProductionTurnPreparationError::InvalidField(
                "workspace_root",
            ));
        }
        Ok(Self {
            turn_id,
            thread_id,
            objective,
            workspace_root,
        })
    }

    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    pub fn objective(&self) -> &str {
        &self.objective
    }

    pub fn workspace_root(&self) -> &std::path::Path {
        &self.workspace_root
    }
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
    reject_empty: bool,
) -> Result<(), ProductionTurnPreparationError> {
    if (reject_empty && value.trim().is_empty()) || value.len() > max_bytes {
        return Err(ProductionTurnPreparationError::InvalidField(field));
    }
    Ok(())
}

/// Supplies bounded, already-approved context for one admitted turn.
pub trait ProductionTurnContextProvider: Send + Sync {
    /// Captures context without reading mutable provider, dashboard, or observation state.
    fn capture(
        &self,
        input: &ProductionTurnAdmissionInput,
    ) -> Result<Option<String>, ProductionTurnPreparationError>;
}

/// An explicit objective-only context provider for tests and integrations that do not require
/// conversation context. It is not installed as an app-server default.
#[derive(Clone, Copy, Debug, Default)]
pub struct ObjectiveOnlyProductionTurnContext;

impl ProductionTurnContextProvider for ObjectiveOnlyProductionTurnContext {
    fn capture(
        &self,
        _input: &ProductionTurnAdmissionInput,
    ) -> Result<Option<String>, ProductionTurnPreparationError> {
        Ok(None)
    }
}

/// Errors raised before a production runner is allowed to spawn work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductionTurnPreparationError {
    RunnerUnavailable,
    ContextUnavailable,
    PolicyUnavailable,
    RoutingUnavailable,
    InvalidField(&'static str),
    ContextTooLarge,
}

impl fmt::Display for ProductionTurnPreparationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RunnerUnavailable => formatter.write_str("production runner is unavailable"),
            Self::ContextUnavailable => {
                formatter.write_str("approved production context is unavailable")
            }
            Self::PolicyUnavailable => {
                formatter.write_str("production execution policy is unavailable")
            }
            Self::RoutingUnavailable => formatter.write_str("production routing is unavailable"),
            Self::InvalidField(field) => write!(formatter, "production field is invalid: {field}"),
            Self::ContextTooLarge => {
                formatter.write_str("approved production context is too large")
            }
        }
    }
}

impl std::error::Error for ProductionTurnPreparationError {}

/// Errors returned after a prepared production run is started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductionTurnRunError {
    EventDestinationClosed,
    RunnerFailed,
    CoordinatorJoinFailure,
    ObservationBridgeJoinFailure,
    ShutdownTimedOut,
    FinalDeliverableFailed,
}

impl fmt::Display for ProductionTurnRunError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EventDestinationClosed => {
                formatter.write_str("production event destination closed")
            }
            Self::RunnerFailed => formatter.write_str("production runner failed"),
            Self::CoordinatorJoinFailure => {
                formatter.write_str("production coordinator join failed")
            }
            Self::ObservationBridgeJoinFailure => {
                formatter.write_str("production observation bridge join failed")
            }
            Self::ShutdownTimedOut => formatter.write_str("production shutdown timed out"),
            Self::FinalDeliverableFailed => {
                formatter.write_str("production final deliverable failed")
            }
        }
    }
}

impl std::error::Error for ProductionTurnRunError {}

pub type ProductionTurnFuture =
    Pin<Box<dyn Future<Output = Result<(), ProductionTurnRunError>> + Send + 'static>>;

/// Prepared work plus the exact cancellation handle and future owned by its caller.
pub struct PreparedProductionTurn {
    cancellation: ProductionOrchestrationCancellationHandle,
    completion: ProductionTurnFuture,
}

impl PreparedProductionTurn {
    pub fn new(
        cancellation: ProductionOrchestrationCancellationHandle,
        completion: ProductionTurnFuture,
    ) -> Self {
        Self {
            cancellation,
            completion,
        }
    }

    pub fn cancellation_handle(&self) -> ProductionOrchestrationCancellationHandle {
        self.cancellation.clone()
    }

    pub fn into_completion(self) -> ProductionTurnFuture {
        self.completion
    }

    pub fn request_cancel(&self, reason: ProductionCancellationReason) -> bool {
        self.cancellation.request_cancel(reason)
    }
}

/// Neutral factory implemented by a trusted in-process production runner.
pub trait ProductionTurnRunnerFactory: Send + Sync {
    /// Validates and prepares one run without spawning coordinator or bridge tasks.
    fn prepare(
        &self,
        input: ProductionTurnAdmissionInput,
        context: Option<String>,
        events: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<PreparedProductionTurn, ProductionTurnPreparationError>;
}

/// Non-serialized runtime dependencies available only to trusted in-process callers.
pub struct ProductionOrchestrationRuntime {
    runner: Arc<dyn ProductionTurnRunnerFactory>,
    context_provider: Arc<dyn ProductionTurnContextProvider>,
}

impl fmt::Debug for ProductionOrchestrationRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProductionOrchestrationRuntime")
            .field("runner", &"<redacted>")
            .field("context_provider", &"<redacted>")
            .finish()
    }
}

impl ProductionOrchestrationRuntime {
    pub fn new(
        runner: Arc<dyn ProductionTurnRunnerFactory>,
        context_provider: Arc<dyn ProductionTurnContextProvider>,
    ) -> Self {
        Self {
            runner,
            context_provider,
        }
    }

    pub fn prepare(
        &self,
        input: ProductionTurnAdmissionInput,
        events: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<PreparedProductionTurn, ProductionTurnPreparationError> {
        let context = self.context_provider.capture(&input)?;
        if context
            .as_ref()
            .is_some_and(|value| value.len() > MAX_PRODUCTION_CONTEXT_BYTES)
        {
            return Err(ProductionTurnPreparationError::ContextTooLarge);
        }
        self.runner.prepare(input, context, events)
    }

    pub fn prepare_optional(
        runtime: Option<&Self>,
        input: ProductionTurnAdmissionInput,
        events: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<PreparedProductionTurn, ProductionTurnPreparationError> {
        runtime.map_or(
            Err(ProductionTurnPreparationError::RunnerUnavailable),
            |runtime| runtime.prepare(input, events),
        )
    }

    pub fn runner(&self) -> &Arc<dyn ProductionTurnRunnerFactory> {
        &self.runner
    }
}

#[cfg(test)]
#[path = "production_runner_tests.rs"]
mod tests;
