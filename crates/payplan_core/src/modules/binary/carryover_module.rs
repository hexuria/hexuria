use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, CoreResult};
use crate::modules::binary::carryover::BinaryCarryover;
use crate::modules::binary::volume::BinaryLegTotals;
use crate::payplan::events::EventType;
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::Module;
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
        &[EventType::BinaryCycleClosed]
    }

    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        let mut state: CarryoverState = ctx.decode_state().map_err(CoreError::from)?;
        let mut result = ModuleResult::empty();

        let Some(event) = &ctx.triggering_event else {
            return Ok(result);
        };
        if event.event_type != EventType::BinaryCycleClosed {
            return Ok(result);
        }

        // Compute new carryover from the cycle's unmatched volume.
        let left_unmatched = state.last_unmatched.left;
        let right_unmatched = state.last_unmatched.right;
        let mut next = BinaryCarryover::from_unmatched(left_unmatched, right_unmatched);
        // Carry the projection keys through: company from ctx, node_id from the
        // event payload when present (resolved fully in Track B).
        next.company_id = Some(ctx.company_id);
        next.node_id = event
            .payload
            .get("node_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .map(BinaryNodeId::from)
            .or(state.carry.node_id);

        result.emit(
            Some(ctx.company_id),
            EventType::BinaryCarryoverUpdated,
            json!({
                "left": next.left_volume,
                "right": next.right_volume,
            }),
        );

        state.carry = next;
        state.last_unmatched = BinaryLegTotals::default();

        result.set_state(
            serde_json::to_value(&state).map_err(|e| CoreError::Validation(e.to_string()))?,
        );
        Ok(result)
    }
}
