use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, CoreResult};
use crate::modules::royal::flushline::{
    RoyalFlushlineAccount, RoyalFlushlineConfig, ROYAL_GRADUATION_POINTS,
};
use crate::payplan::events::{DomainEvent, EventType};
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::Module;
use crate::shared::ids::{EnrollmentId, RoyalAccountId, UserId};

/// Per-(enrollment) persisted state for the Flushline module.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlushlineState {
    #[serde(default)]
    pub account: Option<RoyalFlushlineAccount>,
}

pub struct RoyalFlushlineModule {
    #[allow(dead_code)]
    config: RoyalFlushlineConfig,
}

impl RoyalFlushlineModule {
    #[must_use]
    pub fn new(config: RoyalFlushlineConfig) -> Self {
        Self { config }
    }
}

impl Module for RoyalFlushlineModule {
    fn key(&self) -> &'static str {
        "royal.flushline"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn handles(&self) -> &'static [EventType] {
        &[
            EventType::EnrollmentCreated,
            EventType::PackagePurchased,
            EventType::RoyalFlushlineAccountCreated,
        ]
    }

    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        let mut state: FlushlineState = ctx.decode_state().map_err(CoreError::from)?;
        let mut result = ModuleResult::empty();

        let Some(enrollment_id) = ctx.enrollment_id else {
            result.warn("royal.flushline: no enrollment_id in context, skipping");
            return Ok(result);
        };

        // Lazy-create the account on first enrollment/purchase.
        if state.account.is_none() {
            let owner_user_id = extract_owner_user_id(ctx)?;
            let mut account =
                RoyalFlushlineAccount::new(enrollment_id, owner_user_id);
            account.created_at = ctx.now;
            result.emit(
                EventType::RoyalFlushlineAccountCreated,
                json!({
                    "royal_account_id": account.id,
                    "enrollment_id": account.enrollment_id,
                    "owner_user_id": account.owner_user_id,
                    "starting_tier": account.current_tier,
                }),
            );
            state.account = Some(account);
        }

        // Apply point grants coming from the triggering event.
        if let Some(event) = &ctx.triggering_event {
            if event.event_type == EventType::PackagePurchased {
                let points = event_grant_points(event);
                if let Some(account) = state.account.as_mut() {
                    if !account.graduated {
                        let before_graduated = account.graduated;
                        let before_tier = account.current_tier;
                        let next = account.apply_points(points);
                        let graduated_now = !before_graduated && next.graduated;
                        if graduated_now {
                            result.emit(
                                EventType::RoyalFlushlineGraduated,
                                json!({
                                    "royal_account_id": next.id,
                                    "enrollment_id": next.enrollment_id,
                                    "total_points": next.current_points,
                                    "graduation_threshold": ROYAL_GRADUATION_POINTS,
                                }),
                            );
                        } else if next.current_tier != before_tier {
                            result.emit(
                                EventType::RoyalFlushlineAccountCreated,
                                json!({
                                    "royal_account_id": next.id,
                                    "enrollment_id": next.enrollment_id,
                                    "owner_user_id": next.owner_user_id,
                                    "tier": next.current_tier,
                                    "current_points": next.current_points,
                                }),
                            );
                        }
                        *account = next;
                    }
                }
            }
        }

        if let Some(account) = &state.account {
            result.set_state(
                serde_json::to_value(&FlushlineState {
                    account: Some(account.clone()),
                })
                .map_err(|e| CoreError::Validation(e.to_string()))?,
            );
        }

        Ok(result)
    }
}

fn extract_owner_user_id(ctx: &ModuleContext) -> CoreResult<UserId> {
    let Some(event) = &ctx.triggering_event else {
        return Err(CoreError::Validation(
            "royal.flushline: missing triggering event".into(),
        ));
    };
    for key in ["user_id", "owner_user_id"] {
        if let Some(uid) = event.payload.get(key).and_then(|v| v.as_str()) {
            if let Ok(parsed) = uuid::Uuid::parse_str(uid) {
                return Ok(UserId::from(parsed));
            }
        }
    }
    Err(CoreError::Validation(
        "royal.flushline: triggering event missing user_id/owner_user_id".into(),
    ))
}

fn event_grant_points(event: &DomainEvent) -> u32 {
    event
        .payload
        .get("points")
        .and_then(serde_json::Value::as_u64)
        .map_or(0, |n| u32::try_from(n).unwrap_or(u32::MAX))
}

impl From<serde_json::Error> for CoreError {
    fn from(err: serde_json::Error) -> Self {
        CoreError::Validation(err.to_string())
    }
}

// Suppress unused import warning when nothing else uses it.
#[allow(dead_code)]
const _ENROLLMENT_ID_TYPE_CHECK: Option<EnrollmentId> = None;
#[allow(dead_code)]
const _ROYAL_ACCOUNT_ID_TYPE_CHECK: Option<RoyalAccountId> = None;
