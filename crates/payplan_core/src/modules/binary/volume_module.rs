use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, CoreResult};
use crate::modules::binary::tree::BinaryLeg;
use crate::modules::binary::volume::{
    BinaryLegTotals, BinaryVolumeConfig, BinaryVolumeEntry, BinaryVolumeStatus,
};
use crate::payplan::events::EventType;
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::Module;
use crate::shared::ids::{BinaryNodeId, PurchaseId};

/// Per-(node) accumulated volume ledger.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinaryVolumeState {
    #[serde(default)]
    pub entries: Vec<BinaryVolumeEntry>,
    #[serde(default)]
    pub totals: BinaryLegTotals,
}

pub struct BinaryVolumeModule {
    config: BinaryVolumeConfig,
}

impl BinaryVolumeModule {
    #[must_use]
    pub fn new(config: BinaryVolumeConfig) -> Self {
        Self { config }
    }
}

impl Module for BinaryVolumeModule {
    fn key(&self) -> &'static str {
        "binary.volume"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn handles(&self) -> &'static [EventType] {
        &[EventType::PackagePurchased, EventType::SubscriptionRenewed]
    }

    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        if !self.config.count_purchase_volume {
            return Ok(ModuleResult::empty());
        }

        let mut state: BinaryVolumeState = ctx.decode_state().map_err(CoreError::from)?;
        let mut result = ModuleResult::empty();

        let Some(event) = &ctx.triggering_event else {
            return Ok(result);
        };
        if event.event_type == EventType::SubscriptionRenewed && !self.config.count_renewal_volume {
            return Ok(result);
        }

        let Some(volume) = event
            .payload
            .get("volume")
            .and_then(serde_json::Value::as_i64)
        else {
            return Ok(result);
        };
        let Some(leg_str) = event.payload.get("leg").and_then(|v| v.as_str()) else {
            return Ok(result);
        };
        let leg = match leg_str {
            "left" => BinaryLeg::Left,
            "right" => BinaryLeg::Right,
            _ => return Ok(result),
        };
        let purchase_id = event
            .payload
            .get("purchase_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .map(PurchaseId::from);

        // Recover the node the volume applies to. Not every triggering event
        // carries one (e.g. `PackagePurchased` runs before `BinaryNodePlaced`
        // is processed in the same cascade); rows with `None` are skipped by
        // the projector until node linkage is resolved.
        let node_id = event
            .payload
            .get("node_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .map(BinaryNodeId::from);

        let entry = BinaryVolumeEntry {
            id: uuid::Uuid::now_v7(),
            company_id: ctx.company_id,
            source_purchase_id: purchase_id,
            leg,
            volume,
            status: BinaryVolumeStatus::Open,
            node_id,
        };
        state.entries.push(entry);
        state.totals = state.totals.add(leg, volume);

        result.emit(
            Some(ctx.company_id),
            EventType::BinaryVolumeAdded,
            json!({
                "leg": leg_str,
                "volume": volume,
                "purchase_id": purchase_id,
                "totals": { "left": state.totals.left, "right": state.totals.right },
            }),
        );

        result.set_state(
            serde_json::to_value(&state).map_err(|e| CoreError::Validation(e.to_string()))?,
        );
        Ok(result)
    }
}
