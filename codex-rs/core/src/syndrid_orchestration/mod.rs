//! Core-owned implementation of the narrow Syndrid O2A runtime boundary.
//!
//! This module delegates child creation and message delivery to the existing `AgentControl`.
//! It owns no thread, graph, persistence, token, or event state of its own.

use crate::AgentControl;
use crate::config::Config;
use codex_orchestration_adapter::AdapterError;
use codex_orchestration_adapter::DeliverHandoffRequest;
use codex_orchestration_adapter::DeliverHandoffResult;
use codex_orchestration_adapter::SpawnChildRequest;
use codex_orchestration_adapter::SpawnChildResult;
use codex_protocol::ThreadId;
use codex_protocol::protocol::SessionSource;

mod error;
mod handoff;
mod spawn;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

/// Core-owned bridge for the O2A spawn and handoff contracts.
///
/// The caller supplies the current native parent context and effective base configuration. The
/// bridge delegates lifecycle, graph, persistence, permission enforcement, and event work to the
/// existing Codex runtime through `AgentControl`.
pub(crate) struct CodexOrchestrationAdapter {
    agent_control: AgentControl,
    base_config: Config,
    parent_thread_id: ThreadId,
    parent_session_source: SessionSource,
}

impl CodexOrchestrationAdapter {
    pub(crate) fn new(
        agent_control: AgentControl,
        base_config: Config,
        parent_thread_id: ThreadId,
        parent_session_source: SessionSource,
    ) -> Self {
        Self {
            agent_control,
            base_config,
            parent_thread_id,
            parent_session_source,
        }
    }

    pub(crate) async fn spawn_child(
        &self,
        request: SpawnChildRequest,
    ) -> Result<SpawnChildResult, AdapterError> {
        spawn::spawn_child(self, request).await
    }

    pub(crate) async fn deliver_handoff(
        &self,
        request: DeliverHandoffRequest,
    ) -> Result<DeliverHandoffResult, AdapterError> {
        handoff::deliver_handoff(self, request).await
    }
}
