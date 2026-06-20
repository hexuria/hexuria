use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, CoreResult};
use crate::modules::royal::duplication::{RoyalDuplicationConfig, RoyalDuplicationState};
use crate::payplan::events::EventType;
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::Module;
use crate::shared::ids::{EnrollmentId, RoyalAccountId, UserId};

/// Per-(user) state tracking whether the user has both signals needed to duplicate.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DuplicationState {
    #[serde(default)]
    pub flags: Vec<UserFlag>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFlag {
    pub user_id: UserId,
    pub graduation_seen: bool,
    pub cycle_seen: bool,
    pub duplicated: bool,
}

pub struct RoyalAccountDuplicationModule {
    config: RoyalDuplicationConfig,
}

impl RoyalAccountDuplicationModule {
    #[must_use]
    pub fn new(config: RoyalDuplicationConfig) -> Self {
        Self { config }
    }
}

impl Module for RoyalAccountDuplicationModule {
    fn key(&self) -> &'static str {
        "royal.account_duplication"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn handles(&self) -> &'static [EventType] {
        &[
            EventType::RoyalFlushlineGraduated,
            EventType::RoyalMatrixCycled,
        ]
    }

    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        if !self.config.enabled {
            return Ok(ModuleResult::empty());
        }

        let mut state: DuplicationState = ctx.decode_state().map_err(CoreError::from)?;
        let mut result = ModuleResult::empty();

        let Some(event) = &ctx.triggering_event else {
            return Ok(result);
        };

        let owner = event
            .payload
            .get("owner_user_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .map(UserId::from);

        match event.event_type {
            EventType::RoyalFlushlineGraduated => {
                if let Some(user) = owner {
                    bump(&mut state, user, true, false);
                }
            }
            EventType::RoyalMatrixCycled => {
                if let Some(user) = owner {
                    bump(&mut state, user, false, true);
                }
            }
            _ => {}
        }

        // After state update, attempt duplication for any user now ready.
        let mut new_accounts: Vec<(UserId, RoyalDuplicationState)> = vec![];
        for flag in state.flags.iter_mut() {
            if flag.duplicated {
                continue;
            }
            let ready = flag.graduation_seen && flag.cycle_seen;
            if ready {
                flag.duplicated = true;
                new_accounts.push((
                    flag.user_id,
                    RoyalDuplicationState {
                        flushline_graduated: true,
                        matrix_cycled: true,
                    },
                ));
            }
        }

        for (user_id, _state) in new_accounts {
            // We do NOT create the new RoyalFlushlineAccount here (that would couple us to that module's
            // state schema). We emit the event with enough metadata for downstream consumers (or
            // an engine-side orchestrator) to materialize the account.
            result.emit(
                Some(ctx.company_id),
                EventType::RoyalAccountDuplicated,
                json!({
                    "owner_user_id": user_id,
                    "company_id": ctx.company_id,
                    "package_id": ctx.package_id,
                    "source_enrollment_id": ctx.enrollment_id,
                    "new_royal_account_id": RoyalAccountId::new(),
                }),
            );
        }

        if !state.flags.is_empty() {
            result.set_state(
                serde_json::to_value(&state).map_err(|e| CoreError::Validation(e.to_string()))?,
            );
        }

        let _ = std::marker::PhantomData::<EnrollmentId>;
        Ok(result)
    }
}

fn bump(state: &mut DuplicationState, user: UserId, grad: bool, cycle: bool) {
    if let Some(f) = state.flags.iter_mut().find(|f| f.user_id == user) {
        if grad {
            f.graduation_seen = true;
        }
        if cycle {
            f.cycle_seen = true;
        }
    } else {
        state.flags.push(UserFlag {
            user_id: user,
            graduation_seen: grad,
            cycle_seen: cycle,
            duplicated: false,
        });
    }
}
