use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, CoreResult};
use crate::modules::binary::pairing::{compute_pairing, BinaryPairingConfig, BinaryPairingOutcome};
use crate::modules::binary::volume::BinaryLegTotals;
use crate::payplan::events::EventType;
use crate::payplan::ledger::{LedgerStatus, RewardLedgerEntry};
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::Module;
use crate::shared::ids::{LedgerEntryId, UserId};
use crate::shared::money::Money;

/// Per-(node) pairing state.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BinaryPairingState {
    /// Totals accumulated since the last cycle close. Pairing is run against these.
    #[serde(default)]
    pub pending_totals: BinaryLegTotals,
}

pub struct BinaryPairingModule {
    config: BinaryPairingConfig,
}

impl BinaryPairingModule {
    #[must_use]
    pub fn new(config: BinaryPairingConfig) -> Self {
        Self { config }
    }
}

impl Module for BinaryPairingModule {
    fn key(&self) -> &'static str {
        "binary.pairing_bonus"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn handles(&self) -> &'static [EventType] {
        &[EventType::BinaryVolumeAdded, EventType::BinaryCycleClosed]
    }

    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        let mut state: BinaryPairingState = ctx.decode_state().map_err(CoreError::from)?;
        let mut result = ModuleResult::empty();

        let Some(event) = &ctx.triggering_event else {
            return Ok(result);
        };

        match event.event_type {
            EventType::BinaryVolumeAdded => {
                if let (Some(leg), Some(v)) = (
                    event.payload.get("leg").and_then(|v| v.as_str()),
                    event
                        .payload
                        .get("volume")
                        .and_then(serde_json::Value::as_i64),
                ) {
                    state.pending_totals = match leg {
                        "left" => state
                            .pending_totals
                            .add(crate::modules::binary::tree::BinaryLeg::Left, v),
                        "right" => state
                            .pending_totals
                            .add(crate::modules::binary::tree::BinaryLeg::Right, v),
                        _ => state.pending_totals,
                    };
                }
            }
            EventType::BinaryCycleClosed => {
                let outcome: BinaryPairingOutcome = compute_pairing(
                    state.pending_totals.left,
                    state.pending_totals.right,
                    &self.config,
                );

                let user_id = event
                    .payload
                    .get("node_user_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                    .map(UserId::from);

                // Propagate node_id and period_id from the triggering
                // BinaryCycleClosed event so the event projector can
                // materialise binary_pairing_results without a DB lookup.
                let node_id = event
                    .payload
                    .get("node_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok());
                let period_id = event
                    .payload
                    .get("period_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok());

                let Some(user_id) = user_id else {
                    result.warn("binary.pairing: cycle close event missing node_user_id");
                    state.pending_totals = BinaryLegTotals::default();
                    result.set_state(
                        serde_json::to_value(&state)
                            .map_err(|e| CoreError::Validation(e.to_string()))?,
                    );
                    return Ok(result);
                };

                let ts: DateTime<Utc> = ctx.now;
                let mut pair_payload = json!({
                    "node_user_id": user_id,
                    "left": state.pending_totals.left,
                    "right": state.pending_totals.right,
                    "matched": outcome.matched_volume,
                });
                if let Some(nid) = node_id {
                    pair_payload.as_object_mut().unwrap().insert("node_id".into(), json!(nid));
                }
                if let Some(pid) = period_id {
                    pair_payload.as_object_mut().unwrap().insert("period_id".into(), json!(pid));
                }
                result.emit(
                    Some(ctx.company_id),
                    EventType::BinaryPairMatched,
                    pair_payload,
                );

                if !outcome.commission.is_zero() {
                    result.emit(
                        Some(ctx.company_id),
                        EventType::BinaryCommissionEarned,
                        json!({
                            "node_user_id": user_id,
                            "amount": outcome.commission.to_string(),
                            "capped": outcome.capped,
                        }),
                    );
                    result.ledger_entries.push(RewardLedgerEntry {
                        id: LedgerEntryId::new(),
                        company_id: ctx.company_id,
                        user_id,
                        enrollment_id: ctx.enrollment_id,
                        package_id: Some(ctx.package_id),
                        source_module: "binary.pairing_bonus".into(),
                        source_event_id: ctx.triggering_event.as_ref().map(|e| e.id),
                        amount: Money::new(outcome.commission, "USD"),
                        points: 0,
                        status: LedgerStatus::Pending,
                        reason: "binary.pairing.commission".into(),
                        created_at: ts,
                    });
                }

                state.pending_totals = BinaryLegTotals::default();
            }
            _ => {}
        }

        result.set_state(
            serde_json::to_value(&state).map_err(|e| CoreError::Validation(e.to_string()))?,
        );
        Ok(result)
    }
}

// Decimal typecheck - keep Decimal import live.
#[allow(dead_code)]
const _DECIMAL: Decimal = Decimal::ZERO;
