//! Trusted construction of the non-serialized production session runtime.
//!
//! This module is the composition boundary for an embedded caller that already
//! owns validated production dependencies. It deliberately does not resolve
//! policy, routing, provider connections, accounts, or tools. Those authorities
//! remain in the trusted composition root that supplies the concrete runner
//! factory.

use std::fmt;
use std::sync::Arc;

use codex_app_server::ProductionOrchestrationRuntime;
use codex_app_server::ProductionSessionRuntime;
use codex_app_server::ProductionTurnContextProvider;
use codex_app_server::ProductionTurnRunnerFactory;
use codex_app_server::in_process::InProcessServerEvent;
use tokio::sync::mpsc;

/// Already-validated, trusted inputs used to construct one session runtime.
///
/// The concrete runner factory is expected to be backed by the existing
/// `ProductionOrchestrationTurnRunnerFactory` and to own the immutable policy,
/// routing, provider, approved-tool, budget, deadline, and deliverable
/// dependencies. This container stores only trait-object references and a
/// session identity; it never stores credentials or serialized protocol data.
pub struct TrustedProductionRuntimeDependencies {
    /// Stable identity of the in-process session that will own the runtime.
    pub session_id: String,
    /// Trusted factory containing the existing concrete production runner.
    pub runner_factory: Option<Arc<dyn ProductionTurnRunnerFactory>>,
    /// Trusted bounded context source for admitted production turns.
    pub context_provider: Option<Arc<dyn ProductionTurnContextProvider>>,
}

impl fmt::Debug for TrustedProductionRuntimeDependencies {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrustedProductionRuntimeDependencies")
            .field("session_id", &"<redacted>")
            .field(
                "runner_factory",
                &self.runner_factory.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "context_provider",
                &self.context_provider.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Typed failures raised before a trusted session runtime is constructed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustedRuntimeConstructionError {
    /// The supplied session identity is empty or exceeds the admission bound.
    InvalidSessionIdentity,
    /// The trusted concrete production runner factory was not supplied.
    RunnerUnavailable,
    /// The bounded approved context provider was not supplied.
    ContextProviderUnavailable,
}

impl fmt::Display for TrustedRuntimeConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSessionIdentity => {
                formatter.write_str("trusted production session identity is invalid")
            }
            Self::RunnerUnavailable => {
                formatter.write_str("trusted production runner factory is unavailable")
            }
            Self::ContextProviderUnavailable => {
                formatter.write_str("trusted production context provider is unavailable")
            }
        }
    }
}

impl std::error::Error for TrustedRuntimeConstructionError {}

/// Builds one session-scoped production runtime without starting execution.
///
/// Construction consumes only trusted, already-validated dependencies. It
/// creates no tasks, invokes no providers or tools, emits no events, and does
/// not register cancellation or alter app-server busy state.
#[derive(Debug)]
pub struct TrustedProductionRuntimeBuilder {
    dependencies: TrustedProductionRuntimeDependencies,
}

impl TrustedProductionRuntimeBuilder {
    /// Creates a builder from trusted in-process dependencies.
    pub fn new(dependencies: TrustedProductionRuntimeDependencies) -> Self {
        Self { dependencies }
    }

    /// Constructs a non-serialized runtime bound to the supplied event sink.
    pub fn build(
        self,
        events: mpsc::Sender<InProcessServerEvent>,
    ) -> Result<ProductionSessionRuntime, TrustedRuntimeConstructionError> {
        let TrustedProductionRuntimeDependencies {
            session_id,
            runner_factory,
            context_provider,
        } = self.dependencies;

        validate_session_identity(&session_id)?;
        let runner = runner_factory.ok_or(TrustedRuntimeConstructionError::RunnerUnavailable)?;
        let context_provider =
            context_provider.ok_or(TrustedRuntimeConstructionError::ContextProviderUnavailable)?;
        let runtime = Arc::new(ProductionOrchestrationRuntime::new(
            runner,
            context_provider,
        ));
        Ok(ProductionSessionRuntime::new(session_id, runtime, events))
    }
}

fn validate_session_identity(session_id: &str) -> Result<(), TrustedRuntimeConstructionError> {
    if session_id.trim().is_empty() || session_id.len() > 256 {
        return Err(TrustedRuntimeConstructionError::InvalidSessionIdentity);
    }
    Ok(())
}

#[cfg(test)]
#[path = "trusted_runtime_tests.rs"]
mod tests;
