use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, CoreResult};
use crate::modules::royal::matrix::{RoyalMatrix, RoyalMatrixConfig, RoyalSlot};
use crate::payplan::events::EventType;
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::Module;
use crate::shared::ids::RoyalAccountId;

/// Per-(royal_account) persisted state for the Matrix module.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MatrixState {
    #[serde(default)]
    pub matrices: Vec<RoyalMatrix>,
}

pub struct RoyalMatrixModule {
    config: RoyalMatrixConfig,
}

impl RoyalMatrixModule {
    #[must_use]
    pub fn new(config: RoyalMatrixConfig) -> Self {
        Self { config }
    }
}

impl Module for RoyalMatrixModule {
    fn key(&self) -> &'static str {
        "royal.matrix"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn handles(&self) -> &'static [EventType] {
        &[
            EventType::RoyalFlushlineAccountCreated,
            EventType::RoyalMatrixCreated,
            EventType::RoyalFlushlineGraduated,
        ]
    }

    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        let mut state: MatrixState = ctx.decode_state().map_err(CoreError::from)?;
        let mut result = ModuleResult::empty();

        let Some(event) = &ctx.triggering_event else {
            return Ok(result);
        };

        match event.event_type {
            EventType::RoyalFlushlineAccountCreated => {
                let Some(account_id) = event
                    .payload
                    .get("royal_account_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                else {
                    result.warn("royal.matrix: missing royal_account_id in payload");
                    return Ok(result);
                };
                let owner = RoyalAccountId::from(account_id);
                let needs_new_matrix =
                    state.matrices.is_empty() || state.matrices.last().is_none_or(|m| m.is_full());
                if needs_new_matrix {
                    let mut matrix = RoyalMatrix::new(ctx.company_id, owner);
                    matrix.created_at = ctx.now;
                    state.matrices.push(matrix.clone());
                    result.emit(
                        Some(ctx.company_id),
                        EventType::RoyalMatrixCreated,
                        json!({
                            "matrix_id": matrix.id,
                            "owner_account_id": matrix.owner_account_id,
                            "company_id": matrix.company_id,
                        }),
                    );
                }
            }
            EventType::RoyalMatrixCreated => {
                // No-op; matrix creation is handled in the other branch.
            }
            EventType::RoyalFlushlineGraduated => {
                if let Some(last) = state.matrices.last_mut() {
                    if last.is_full() {
                        last.cycle();
                        result.emit(
                            Some(ctx.company_id),
                            EventType::RoyalMatrixCycled,
                            json!({
                                "matrix_id": last.id,
                                "cycle_count": last.cycle_count,
                            }),
                        );
                        if self.config.auto_cycle {
                            // After a cycle, the next matrix is created lazily on the next
                            // RoyalFlushlineAccountCreated event, so we just record state here.
                        }
                    }
                }
            }
            _ => {}
        }

        if !state.matrices.is_empty() {
            result.set_state(
                serde_json::to_value(&state).map_err(|e| CoreError::Validation(e.to_string()))?,
            );
        }

        // Quietly consume the slot enum so the import is preserved when this file grows.
        let _ = RoyalSlot::S1;

        Ok(result)
    }
}
