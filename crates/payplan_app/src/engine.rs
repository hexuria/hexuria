use payplan_core::payplan::events::DomainEvent;
use payplan_core::payplan::ledger::RewardLedgerEntry;

#[derive(Debug, Default)]
pub struct PayPlanEngineResult {
    pub emitted_events: Vec<DomainEvent>,
    pub ledger_entries: Vec<RewardLedgerEntry>,
}

#[derive(Debug, Default)]
pub struct PayPlanEngine;

impl PayPlanEngine {
    pub fn new() -> Self {
        Self
    }
}
