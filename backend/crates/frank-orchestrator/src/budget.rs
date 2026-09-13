//! Token, cost and turn accounting, and the budget ceilings it is checked
//! against.

use std::collections::HashMap;

use frank_agent::UsageTelemetry;
use frank_protocol::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsageTotals {
    pub measured_input_tokens: u64,
    pub measured_output_tokens: u64,
    pub estimated_input_tokens: u64,
    pub estimated_output_tokens: u64,
    pub cost_micros: u64,
}

impl UsageTotals {
    pub fn add(&mut self, usage: &UsageTelemetry) {
        self.measured_input_tokens = self
            .measured_input_tokens
            .saturating_add(usage.measured_input_tokens.unwrap_or_default());
        self.measured_output_tokens = self
            .measured_output_tokens
            .saturating_add(usage.measured_output_tokens.unwrap_or_default());
        self.estimated_input_tokens = self
            .estimated_input_tokens
            .saturating_add(usage.estimated_input_tokens.unwrap_or_default());
        self.estimated_output_tokens = self
            .estimated_output_tokens
            .saturating_add(usage.estimated_output_tokens.unwrap_or_default());
        self.cost_micros = self
            .cost_micros
            .saturating_add(usage.cost_micros.unwrap_or_default());
    }

    pub fn measured_tokens(&self) -> u64 {
        self.measured_input_tokens
            .saturating_add(self.measured_output_tokens)
    }

    pub fn estimated_tokens(&self) -> u64 {
        self.estimated_input_tokens
            .saturating_add(self.estimated_output_tokens)
    }
}

#[derive(Debug, Clone, Default)]
pub struct BudgetLedger {
    totals: HashMap<String, UsageTotals>,
    /// Count provider usage frames as durable turns.  Kept separate from
    /// `UsageTotals` so the measured/estimated token DTO remains wire
    /// compatible while turn budgets can still be enforced hard.
    turns: HashMap<String, u32>,
}

impl BudgetLedger {
    pub fn record(&mut self, scope_id: impl Into<String>, usage: &UsageTelemetry) {
        let scope_id = scope_id.into();
        self.totals.entry(scope_id.clone()).or_default().add(usage);
        let count = self.turns.entry(scope_id).or_default();
        *count = count.saturating_add(1);
    }

    /// Rebuild the in-memory budget projection from the durable usage rows.
    ///
    /// The ledger is an acceleration structure, not a second source of truth:
    /// a daemon restart must not reset a mission or agent's hard budget. Task
    /// usage is also attributed to its parent mission and assigned agent so
    /// hierarchical limits remain enforceable after recovery.
    pub fn rebuild_from_snapshot(&mut self, snapshot: &Snapshot) {
        self.totals.clear();
        self.turns.clear();
        for usage in &snapshot.usage {
            let telemetry = UsageTelemetry {
                measured_input_tokens: usage.measured_input_tokens,
                measured_output_tokens: usage.measured_output_tokens,
                estimated_input_tokens: usage.estimated_input_tokens,
                estimated_output_tokens: usage.estimated_output_tokens,
                cost_micros: usage.cost_micros,
                cached_input_tokens: usage.cached_input_tokens,
                reasoning_tokens: usage.reasoning_tokens,
            };
            self.record(usage.scope_id.clone(), &telemetry);
            if usage.scope != BudgetScope::Task {
                continue;
            }
            let Ok(task_id) = TaskId::parse(&usage.scope_id) else {
                continue;
            };
            let Some(task) = snapshot.tasks.iter().find(|task| task.id == task_id) else {
                continue;
            };
            self.record(task.mission_id.to_string(), &telemetry);
            if let Some(agent_id) = task.assigned_agent {
                self.record(agent_id.to_string(), &telemetry);
            }
        }
    }

    pub fn totals(&self, scope_id: &str) -> UsageTotals {
        self.totals.get(scope_id).cloned().unwrap_or_default()
    }

    pub fn turns(&self, scope_id: &str) -> u32 {
        self.turns.get(scope_id).copied().unwrap_or_default()
    }

    pub fn allows_measured(&self, scope_id: &str, budget: &Budget) -> bool {
        let totals = self.totals(scope_id);
        budget
            .turns
            .is_none_or(|limit| self.turns(scope_id) <= limit)
            && budget
                .measured_tokens
                .is_none_or(|limit| totals.measured_tokens() <= limit)
            && budget
                .cost_micros
                .is_none_or(|limit| totals.cost_micros <= limit)
    }
}
