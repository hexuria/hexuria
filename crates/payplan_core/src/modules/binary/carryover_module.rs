use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, CoreResult};
use crate::modules::binary::carryover::BinaryCarryover;
use crate::modules::binary::volume::BinaryLegTotals;
use crate::payplan::events::EventType;
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::{AggregateScope, Module};
use crate::shared::ids::BinaryNodeId;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CarryoverState {
    #[serde(default)]
    pub carry: BinaryCarryover,
    /// Last cycle's pending totals, used to compute next carryover if not drained.
    #[serde(default)]
    pub last_unmatched: BinaryLegTotals,
}

pub struct BinaryCarryoverModule;

impl BinaryCarryoverModule {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for BinaryCarryoverModule {
    fn default() -> Self {
        Self::new()
    }
}

impl Module for BinaryCarryoverModule {
    fn key(&self) -> &'static str {
        "binary.carryover"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn handles(&self) -> &'static [EventType] {
        &[EventType::BinaryPairMatched]
    }

    /// Carryover tracks the system-wide binary tree's unmatched leg volume, so
    /// it shares the global aggregate with `binary.tree`.
    fn scope(&self) -> AggregateScope {
        AggregateScope::Global
    }

    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        let mut state: CarryoverState = ctx.decode_state().map_err(CoreError::from)?;
        let mut result = ModuleResult::empty();

        let Some(event) = &ctx.triggering_event else {
            return Ok(result);
        };
        if event.event_type != EventType::BinaryPairMatched {
            return Ok(result);
        }

        // The pairing module emits `left`/`right`/`matched` on every
        // `BinaryPairMatched`. The unmatched remainder (each leg minus the
        // matched volume) is what carries to the next cycle. Previously the
        // carryover read `state.last_unmatched`, which nothing ever wrote, so
        // the carry was always (0,0) and unmatched volume was silently lost
        // (Task 8). We now derive it from the event payload instead.
        let leg_value = |key: &str| event.payload.get(key).and_then(|v| v.as_i64());
        let left = leg_value("left").unwrap_or(0);
        let right = leg_value("right").unwrap_or(0);
        let matched = leg_value("matched").unwrap_or(0);
        let left_unmatched = (left - matched).max(0);
        let right_unmatched = (right - matched).max(0);

        // Accumulate onto any volume already carried from prior cycles so the
        // next cycle opens with the running unmatched balance.
        let mut next = BinaryCarryover::from_unmatched(
            state.carry.left_volume + left_unmatched,
            state.carry.right_volume + right_unmatched,
        );
        // Carry the projection keys through: node_id from the event payload when present
        // (pairing forwards it).
        next.node_id = event
            .payload
            .get("node_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .map(BinaryNodeId::from)
            .or(state.carry.node_id);

        result.emit(
            EventType::BinaryCarryoverUpdated,
            json!({
                "left": next.left_volume,
                "right": next.right_volume,
            }),
        );

        state.carry = next;
        // Record this cycle's remainder for observability/debugging.
        state.last_unmatched = BinaryLegTotals {
            left: left_unmatched,
            right: right_unmatched,
        };

        result.set_state(
            serde_json::to_value(&state).map_err(|e| CoreError::Validation(e.to_string()))?,
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payplan::events::DomainEvent;
    use crate::shared::ids::{EventId, PackageId};
    use serde_json::json;

    fn pair_matched(left: i64, right: i64, matched: i64) -> DomainEvent {
        DomainEvent {
            id: EventId::new(),
            event_type: EventType::BinaryPairMatched,
            payload: json!({ "left": left, "right": right, "matched": matched }),
            created_at: chrono::Utc::now(),
        }
    }

    #[test]
    fn carries_unmatched_remainder_from_pair_event() {
        let module = BinaryCarryoverModule::new();
        let ctx = ModuleContext::new(PackageId::new()).with_event(pair_matched(10, 7, 7));
        let result = module.run(&ctx).expect("run");

        let state: CarryoverState =
            serde_json::from_value(result.state_change.expect("state set")).unwrap();
        assert_eq!(state.carry.left_volume, 3, "unmatched left carries");
        assert_eq!(state.carry.right_volume, 0, "matched right leaves no carry");
        assert!(result
            .emitted_events
            .iter()
            .any(|e| e.event_type == EventType::BinaryCarryoverUpdated));
    }

    #[test]
    fn carryover_accumulates_across_cycles() {
        let module = BinaryCarryoverModule::new();

        // Cycle 1: left=5 over.
        let ctx1 = ModuleContext::new(PackageId::new()).with_event(pair_matched(8, 3, 3));
        let s1: CarryoverState =
            serde_json::from_value(module.run(&ctx1).unwrap().state_change.unwrap()).unwrap();
        assert_eq!(s1.carry.left_volume, 5);

        // Cycle 2: left=2 more over, fed prior carry as state → total left=7.
        let ctx2 = ModuleContext::new(PackageId::new())
            .with_event(pair_matched(4, 2, 2))
            .with_module_state(serde_json::to_value(&s1).unwrap());
        let s2: CarryoverState =
            serde_json::from_value(module.run(&ctx2).unwrap().state_change.unwrap()).unwrap();
        assert_eq!(s2.carry.left_volume, 7, "carry accumulates 5 + 2");
    }
}
