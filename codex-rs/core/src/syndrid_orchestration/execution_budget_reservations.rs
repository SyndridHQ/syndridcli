use super::BudgetExhaustion;
use super::BudgetExhaustionCategory;
use super::ExecutionBudgetLedger;
use super::RoutingRole;

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
