use super::orchestration_failure::OrchestrationFailure;
use super::orchestration_failure::TerminalCauseArbiter;
use super::orchestration_failure::TerminalCauseSubmission;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;

/// The coordinator-owned child categories whose completion is required before terminalization.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum CleanupChildKind {
    Planner,
    ExecutorBatch,
    Verifier,
    Repair,
    Provider,
    Tool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CleanupReservationKind {
    Provider,
    Tool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CleanupSnapshot {
    pub active_children: usize,
    pub active_planner_children: usize,
    pub active_executor_children: usize,
    pub active_verifier_children: usize,
    pub active_repair_children: usize,
    pub active_provider_futures: usize,
    pub active_tool_futures: usize,
    pub unresolved_provider_reservations: usize,
    pub unresolved_tool_reservations: usize,
    pub cleanup_requested: bool,
    pub cleanup_in_progress: bool,
    pub cleanup_complete: bool,
}

impl CleanupSnapshot {
    pub(crate) fn active_children_by_kind(&self, kind: CleanupChildKind) -> usize {
        match kind {
            CleanupChildKind::Provider => self.active_provider_futures,
            CleanupChildKind::Tool => self.active_tool_futures,
            CleanupChildKind::Planner => self.active_planner_children,
            CleanupChildKind::ExecutorBatch => self.active_executor_children,
            CleanupChildKind::Verifier => self.active_verifier_children,
            CleanupChildKind::Repair => self.active_repair_children,
        }
    }

    pub(crate) fn ready_for_terminalization(&self) -> bool {
        self.cleanup_complete
            && self.active_children == 0
            && self.active_provider_futures == 0
            && self.active_tool_futures == 0
            && self.unresolved_provider_reservations == 0
            && self.unresolved_tool_reservations == 0
    }
}

/// Generation-bound ownership for child work and reservations in one live run.
pub(crate) struct OrchestrationCleanup {
    generation: u64,
    state: Mutex<CleanupState>,
    cause_arbiter: TerminalCauseArbiter,
}

struct CleanupState {
    next_handle: u64,
    active_children: BTreeMap<u64, CleanupChildKind>,
    completed_children: BTreeSet<u64>,
    provider_reservations: BTreeSet<u64>,
    tool_reservations: BTreeSet<u64>,
    resolved_provider_reservations: BTreeSet<u64>,
    resolved_tool_reservations: BTreeSet<u64>,
    cleanup_requested: bool,
    cleanup_in_progress: bool,
    cleanup_complete: bool,
}

impl OrchestrationCleanup {
    pub(crate) fn new(generation: u64) -> Self {
        Self {
            generation,
            state: Mutex::new(CleanupState {
                next_handle: 0,
                active_children: BTreeMap::new(),
                completed_children: BTreeSet::new(),
                provider_reservations: BTreeSet::new(),
                tool_reservations: BTreeSet::new(),
                resolved_provider_reservations: BTreeSet::new(),
                resolved_tool_reservations: BTreeSet::new(),
                cleanup_requested: false,
                cleanup_in_progress: false,
                cleanup_complete: false,
            }),
            cause_arbiter: TerminalCauseArbiter::new(generation),
        }
    }

    pub(crate) fn submit_failure(
        &self,
        generation: u64,
        failure: OrchestrationFailure,
    ) -> TerminalCauseSubmission {
        self.cause_arbiter.submit(generation, failure)
    }

    pub(crate) fn freeze_failure(
        &self,
        generation: u64,
    ) -> Result<Option<OrchestrationFailure>, ()> {
        self.cause_arbiter.freeze(generation)
    }

    pub(crate) fn current_failure(
        &self,
        generation: u64,
    ) -> Result<Option<OrchestrationFailure>, ()> {
        self.cause_arbiter.current(generation)
    }

    pub(crate) fn register_child(
        &self,
        generation: u64,
        kind: CleanupChildKind,
    ) -> Result<u64, ()> {
        let Ok(mut state) = self.state.lock() else {
            return Err(());
        };
        if generation != self.generation || state.cleanup_requested || state.cleanup_complete {
            return Err(());
        }
        let handle = next_handle(&mut state)?;
        state.active_children.insert(handle, kind);
        Ok(handle)
    }

    pub(crate) fn register_child_guard(
        self: &Arc<Self>,
        generation: u64,
        kind: CleanupChildKind,
    ) -> Result<CleanupChildGuard, ()> {
        let handle = self.register_child(generation, kind)?;
        Ok(CleanupChildGuard {
            cleanup: Arc::clone(self),
            generation,
            handle: Some(handle),
        })
    }

    pub(crate) fn complete_child(&self, generation: u64, handle: u64) -> Result<(), ()> {
        let Ok(mut state) = self.state.lock() else {
            return Err(());
        };
        if generation != self.generation {
            return Err(());
        }
        if state.completed_children.contains(&handle) {
            return Ok(());
        }
        if state.active_children.remove(&handle).is_none() {
            return Err(());
        }
        state.completed_children.insert(handle);
        Ok(())
    }

    pub(crate) fn register_provider_reservation(&self, generation: u64) -> Result<u64, ()> {
        self.register_reservation(generation, true)
    }

    pub(crate) fn register_provider_reservation_guard(
        self: &Arc<Self>,
        generation: u64,
    ) -> Result<CleanupReservationGuard, ()> {
        self.register_reservation_guard(generation, CleanupReservationKind::Provider)
    }

    pub(crate) fn register_tool_reservation(&self, generation: u64) -> Result<u64, ()> {
        self.register_reservation(generation, false)
    }

    pub(crate) fn register_tool_reservation_guard(
        self: &Arc<Self>,
        generation: u64,
    ) -> Result<CleanupReservationGuard, ()> {
        self.register_reservation_guard(generation, CleanupReservationKind::Tool)
    }

    pub(crate) fn resolve_provider_reservation(
        &self,
        generation: u64,
        handle: u64,
    ) -> Result<(), ()> {
        self.resolve_reservation(generation, handle, true)
    }

    pub(crate) fn resolve_tool_reservation(&self, generation: u64, handle: u64) -> Result<(), ()> {
        self.resolve_reservation(generation, handle, false)
    }

    pub(crate) fn begin(&self, generation: u64) -> Result<(), ()> {
        let Ok(mut state) = self.state.lock() else {
            return Err(());
        };
        if generation != self.generation {
            return Err(());
        }
        if state.cleanup_complete {
            return Ok(());
        }
        if state.cleanup_requested {
            return Ok(());
        }
        state.cleanup_requested = true;
        state.cleanup_in_progress = true;
        Ok(())
    }

    pub(crate) fn complete(&self, generation: u64) -> Result<(), ()> {
        let Ok(mut state) = self.state.lock() else {
            return Err(());
        };
        if generation != self.generation {
            return Err(());
        }
        if state.cleanup_complete {
            return Ok(());
        }
        if !state.cleanup_requested
            || !state.active_children.is_empty()
            || !state.provider_reservations.is_empty()
            || !state.tool_reservations.is_empty()
        {
            return Err(());
        }
        state.cleanup_in_progress = false;
        state.cleanup_complete = true;
        Ok(())
    }

    pub(crate) fn snapshot(&self, generation: u64) -> Result<CleanupSnapshot, ()> {
        let Ok(state) = self.state.lock() else {
            return Err(());
        };
        if generation != self.generation {
            return Err(());
        }
        let active_provider_futures = state
            .active_children
            .values()
            .filter(|kind| **kind == CleanupChildKind::Provider)
            .count();
        let active_tool_futures = state
            .active_children
            .values()
            .filter(|kind| **kind == CleanupChildKind::Tool)
            .count();
        let active_planner_children = state
            .active_children
            .values()
            .filter(|kind| **kind == CleanupChildKind::Planner)
            .count();
        let active_executor_children = state
            .active_children
            .values()
            .filter(|kind| **kind == CleanupChildKind::ExecutorBatch)
            .count();
        let active_verifier_children = state
            .active_children
            .values()
            .filter(|kind| **kind == CleanupChildKind::Verifier)
            .count();
        let active_repair_children = state
            .active_children
            .values()
            .filter(|kind| **kind == CleanupChildKind::Repair)
            .count();
        Ok(CleanupSnapshot {
            active_children: state.active_children.len(),
            active_planner_children,
            active_executor_children,
            active_verifier_children,
            active_repair_children,
            active_provider_futures,
            active_tool_futures,
            unresolved_provider_reservations: state.provider_reservations.len(),
            unresolved_tool_reservations: state.tool_reservations.len(),
            cleanup_requested: state.cleanup_requested,
            cleanup_in_progress: state.cleanup_in_progress,
            cleanup_complete: state.cleanup_complete,
        })
    }

    fn register_reservation(&self, generation: u64, provider: bool) -> Result<u64, ()> {
        let Ok(mut state) = self.state.lock() else {
            return Err(());
        };
        if generation != self.generation || state.cleanup_requested || state.cleanup_complete {
            return Err(());
        }
        let handle = next_handle(&mut state)?;
        if provider {
            state.provider_reservations.insert(handle);
        } else {
            state.tool_reservations.insert(handle);
        }
        Ok(handle)
    }

    fn register_reservation_guard(
        self: &Arc<Self>,
        generation: u64,
        kind: CleanupReservationKind,
    ) -> Result<CleanupReservationGuard, ()> {
        let handle = self
            .register_reservation(generation, matches!(kind, CleanupReservationKind::Provider))?;
        Ok(CleanupReservationGuard {
            cleanup: Arc::clone(self),
            generation,
            kind,
            handle: Some(handle),
        })
    }

    fn resolve_reservation(&self, generation: u64, handle: u64, provider: bool) -> Result<(), ()> {
        let Ok(mut state) = self.state.lock() else {
            return Err(());
        };
        if generation != self.generation {
            return Err(());
        }
        if provider {
            if state.provider_reservations.remove(&handle) {
                state.resolved_provider_reservations.insert(handle);
                return Ok(());
            }
            return if state.resolved_provider_reservations.contains(&handle) {
                Ok(())
            } else {
                Err(())
            };
        }
        if state.tool_reservations.remove(&handle) {
            state.resolved_tool_reservations.insert(handle);
            return Ok(());
        }
        if state.resolved_tool_reservations.contains(&handle) {
            Ok(())
        } else {
            Err(())
        }
    }
}

/// Owns one registered child until the child explicitly completes or the guard is dropped.
///
/// Dropping the guard is the ordinary early-exit cleanup path. It is best effort if the
/// cleanup state is already poisoned; process-abort paths are outside this guarantee.
#[must_use]
pub(crate) struct CleanupChildGuard {
    cleanup: Arc<OrchestrationCleanup>,
    generation: u64,
    handle: Option<u64>,
}

impl CleanupChildGuard {
    pub(crate) fn complete(&mut self) -> Result<(), ()> {
        let Some(handle) = self.handle else {
            return Ok(());
        };
        self.cleanup.complete_child(self.generation, handle)?;
        self.handle = None;
        Ok(())
    }
}

impl Drop for CleanupChildGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle {
            let _ = self.cleanup.complete_child(self.generation, handle);
        }
    }
}

/// Owns one provider or tool reservation until it is resolved or the guard is dropped.
///
/// This guard covers registration failures and other early exits before execution starts.
/// It does not replace the execution-budget reservation, whose commit semantics remain
/// authoritative for accounting.
#[must_use]
pub(crate) struct CleanupReservationGuard {
    cleanup: Arc<OrchestrationCleanup>,
    generation: u64,
    kind: CleanupReservationKind,
    handle: Option<u64>,
}

impl CleanupReservationGuard {
    pub(crate) fn resolve(&mut self) -> Result<(), ()> {
        let Some(handle) = self.handle else {
            return Ok(());
        };
        match self.kind {
            CleanupReservationKind::Provider => self
                .cleanup
                .resolve_provider_reservation(self.generation, handle)?,
            CleanupReservationKind::Tool => self
                .cleanup
                .resolve_tool_reservation(self.generation, handle)?,
        }
        self.handle = None;
        Ok(())
    }
}

impl Drop for CleanupReservationGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.handle {
            let result = match self.kind {
                CleanupReservationKind::Provider => self
                    .cleanup
                    .resolve_provider_reservation(self.generation, handle),
                CleanupReservationKind::Tool => self
                    .cleanup
                    .resolve_tool_reservation(self.generation, handle),
            };
            let _ = result;
        }
    }
}

fn next_handle(state: &mut CleanupState) -> Result<u64, ()> {
    state.next_handle = state.next_handle.checked_add(1).ok_or(())?;
    Ok(state.next_handle)
}
