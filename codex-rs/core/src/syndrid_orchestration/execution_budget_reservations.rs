use super::BudgetExhaustion;
use super::BudgetExhaustionCategory;
use super::ExecutionBudgetLedger;
use super::RoutingRole;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProviderInvocationTerminal {
    Completed,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug)]
pub struct ProviderReservation {
    pub(super) ledger: ExecutionBudgetLedger,
    pub(super) role: RoutingRole,
    pub(super) committed: bool,
}

impl ProviderReservation {
    pub fn commit(mut self) -> Result<(), BudgetExhaustion> {
        let mut state = self.ledger.state.lock().map_err(|_| BudgetExhaustion {
            category: BudgetExhaustionCategory::LedgerUnavailable,
            limit: 0,
            consumed_or_reserved: 0,
            role: Some(self.role),
        })?;
        state.provider_reserved = state.provider_reserved.saturating_sub(1);
        state.provider_started = state.provider_started.saturating_add(1);
        self.committed = true;
        Ok(())
    }

    pub(crate) fn commit_with_guard(mut self) -> Result<ProviderInvocationGuard, BudgetExhaustion> {
        let ledger = self.ledger.clone();
        let mut state = self.ledger.state.lock().map_err(|_| BudgetExhaustion {
            category: BudgetExhaustionCategory::LedgerUnavailable,
            limit: 0,
            consumed_or_reserved: 0,
            role: Some(self.role),
        })?;
        state.provider_reserved = state.provider_reserved.saturating_sub(1);
        state.provider_started = state.provider_started.saturating_add(1);
        self.committed = true;
        Ok(ProviderInvocationGuard {
            ledger,
            terminal: Some(ProviderInvocationTerminal::Failed),
        })
    }
}

impl Drop for ProviderReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Ok(mut state) = self.ledger.state.lock() {
            state.provider_reserved = state.provider_reserved.saturating_sub(1);
            if let Some(value) = state.provider_by_role.get_mut(&self.role) {
                *value = value.saturating_sub(1);
            }
        }
    }
}

/// Owns the terminal accounting for one committed provider invocation.
pub(crate) struct ProviderInvocationGuard {
    ledger: ExecutionBudgetLedger,
    terminal: Option<ProviderInvocationTerminal>,
}

impl ProviderInvocationGuard {
    pub(crate) fn finish(&mut self, terminal: ProviderInvocationTerminal) {
        let Some(_) = self.terminal.take() else {
            return;
        };
        match terminal {
            ProviderInvocationTerminal::Completed => self.ledger.record_provider_completed(),
            ProviderInvocationTerminal::Failed => self.ledger.record_provider_failed(),
            ProviderInvocationTerminal::Cancelled => self.ledger.record_provider_cancelled(),
            ProviderInvocationTerminal::TimedOut => self.ledger.record_provider_timed_out(),
        }
    }
}

impl Drop for ProviderInvocationGuard {
    fn drop(&mut self) {
        if self.terminal.is_some() {
            self.finish(ProviderInvocationTerminal::Failed);
        }
    }
}

#[derive(Debug)]
pub struct ToolReservation {
    pub(super) ledger: ExecutionBudgetLedger,
    pub(super) committed: bool,
}

impl ToolReservation {
    pub fn commit(mut self) -> Result<(), BudgetExhaustion> {
        let mut state = self.ledger.state.lock().map_err(|_| BudgetExhaustion {
            category: BudgetExhaustionCategory::LedgerUnavailable,
            limit: 0,
            consumed_or_reserved: 0,
            role: None,
        })?;
        state.tool_reserved = state.tool_reserved.saturating_sub(1);
        state.tool_started = state.tool_started.saturating_add(1);
        self.committed = true;
        Ok(())
    }
}

impl Drop for ToolReservation {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        if let Ok(mut state) = self.ledger.state.lock() {
            state.tool_reserved = state.tool_reserved.saturating_sub(1);
        }
    }
}
