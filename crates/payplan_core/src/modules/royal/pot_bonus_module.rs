use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::{CoreError, CoreResult};
use crate::modules::royal::flushline::RoyalFlushlineAccount;
use crate::modules::royal::pot_bonus::{distribute, RoyalPotBonusConfig, RoyalQualification};
use crate::payplan::events::EventType;
use crate::payplan::ledger::{LedgerStatus, RewardLedgerEntry};
use crate::payplan::module::{ModuleContext, ModuleResult};
use crate::payplan::registry::{AggregateScope, Module};
use crate::shared::ids::{LedgerEntryId, UserId};
use crate::shared::money::Money;

/// User-level qualification table (one row per user per company).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PotBonusState {
    #[serde(default)]
    pub pool: Decimal,
    #[serde(default)]
    pub qualifications: Vec<RoyalQualification>,
}

pub struct RoyalPotBonusModule {
    config: RoyalPotBonusConfig,
}

impl RoyalPotBonusModule {
    #[must_use]
    pub fn new(config: RoyalPotBonusConfig) -> Self {
        Self { config }
    }
}

impl Module for RoyalPotBonusModule {
    fn key(&self) -> &'static str {
        "royal.pot_bonus"
    }
    fn version(&self) -> &'static str {
        "1.0.0"
    }

    fn handles(&self) -> &'static [EventType] {
        &[
            EventType::RoyalFlushlineGraduated,
            EventType::RoyalMatrixCycled,
            EventType::PackagePurchased,
            EventType::RoyalPotBonusDistributed,
        ]
    }

    /// The royal pot is a single company-wide pool; its qualification table and
    /// balance must be shared across all enrollments in the company.
    fn scope(&self) -> AggregateScope {
        AggregateScope::Company
    }

    fn run(&self, ctx: &ModuleContext) -> CoreResult<ModuleResult> {
        let mut state: PotBonusState = ctx.decode_state().map_err(CoreError::from)?;
        let mut result = ModuleResult::empty();

        let Some(event) = &ctx.triggering_event else {
            return Ok(result);
        };

        match event.event_type {
            EventType::RoyalFlushlineGraduated => {
                if let Some(uid) = event
                    .payload
                    .get("owner_user_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                {
                    let user = UserId::from(uid);
                    bump_qual(&mut state, user, true, false);
                }
                // Per PRD: Flushline graduations contribute points into the pot.
                let pts = event
                    .payload
                    .get("total_points")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                state.pool += Decimal::from(pts);
            }
            EventType::RoyalMatrixCycled => {
                // Cycle events without owner info: cannot credit a specific user.
                // Callers may set `owner_user_id` on the cycle event in the future.
                if let Some(uid) = event
                    .payload
                    .get("owner_user_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
                {
                    let user = UserId::from(uid);
                    bump_qual(&mut state, user, false, true);
                }
            }
            EventType::PackagePurchased => {
                // Purchases contribute a configurable share to the weekly pool.
                let share = event
                    .payload
                    .get("pot_contribution")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0);
                state.pool += Decimal::from(share);
            }
            EventType::RoyalPotBonusDistributed => {
                // Idempotent re-distribution. Run the distribution with the current pool
                // and zero it out.
                let qualified: Vec<UserId> = state
                    .qualifications
                    .iter()
                    .filter(|q| q.is_qualified)
                    .map(|q| q.user_id)
                    .collect();
                let qualified_count = u32::try_from(qualified.len()).unwrap_or(u32::MAX);
                if let Some(outcome) = distribute(state.pool, &self.config, qualified_count) {
                    let ts: DateTime<Utc> = ctx.now;
                    // Per-user breakdown for the emitted event so the event
                    // projector can upsert royal_pot_bonus_balances without a
                    // ledger lookup. Amounts are carried as exact Decimal
                    // strings (Task 11) — the projection columns are NUMERIC, so
                    // no minor-unit truncation (187.5 stays 187.5).
                    let profit_share_str = outcome.per_qualified_user.to_string();
                    let mut distributions: std::collections::BTreeMap<UserId, serde_json::Value> =
                        std::collections::BTreeMap::new();
                    for user_id in &qualified {
                        result.ledger_entries.push(RewardLedgerEntry {
                            id: LedgerEntryId::new(),
                            company_id: ctx.company_id,
                            user_id: *user_id,
                            enrollment_id: ctx.enrollment_id,
                            package_id: Some(ctx.package_id),
                            source_module: "royal.pot_bonus".into(),
                            source_event_id: ctx.triggering_event.as_ref().map(|e| e.id),
                            amount: Money::new(outcome.per_qualified_user, "POINTS"),
                            points: 0,
                            status: LedgerStatus::Pending,
                            reason: "royal.pot_bonus.profit_share".into(),
                            created_at: ts,
                        });
                        distributions.insert(
                            *user_id,
                            json!({
                                "user_id": user_id,
                                "profit_share": profit_share_str,
                            }),
                        );
                    }
                    for (i, payout) in outcome.top_cycler_payouts.into_iter().enumerate() {
                        // Top cyclers are positional: the N-th highest qualifier.
                        if let Some(top) = state
                            .qualifications
                            .iter()
                            .filter(|q| q.is_qualified)
                            .nth(i)
                        {
                            result.ledger_entries.push(RewardLedgerEntry {
                                id: LedgerEntryId::new(),
                                company_id: ctx.company_id,
                                user_id: top.user_id,
                                enrollment_id: ctx.enrollment_id,
                                package_id: Some(ctx.package_id),
                                source_module: "royal.pot_bonus".into(),
                                source_event_id: ctx.triggering_event.as_ref().map(|e| e.id),
                                amount: Money::new(payout, "POINTS"),
                                points: 0,
                                status: LedgerStatus::Pending,
                                reason: format!("royal.pot_bonus.top_cycler[{i}]"),
                                created_at: ts,
                            });
                            let entry = distributions
                                .entry(top.user_id)
                                .or_insert_with(|| json!({ "user_id": top.user_id }));
                            entry
                                .as_object_mut()
                                .expect("distribution entry is an object")
                                .insert("top_cycler".into(), json!(payout.to_string()));
                        }
                    }
                    let distributions_values: Vec<serde_json::Value> =
                        distributions.into_values().collect();
                    // Emit a *terminal* settled event, NOT another
                    // RoyalPotBonusDistributed. Re-emitting the trigger would
                    // re-enter this handler (it's in `handles()`) and, because
                    // the default weights make the payout vec never empty,
                    // `distribute()` keeps returning Some forever → infinite
                    // self-cascade (Path B has no MAX_ITERATIONS backstop
                    // before this fix). RoyalPotBonusSettled is handled by no
                    // module, only by the projector.
                    result.emit(
                        Some(ctx.company_id),
                        EventType::RoyalPotBonusSettled,
                        json!({
                            "pool": state.pool.to_string(),
                            "qualified_count": outcome.qualified_user_count,
                            "per_qualified_user": outcome.per_qualified_user.to_string(),
                            "distributions": distributions_values,
                        }),
                    );
                }
                state.pool = Decimal::ZERO;
            }
            _ => {}
        }

        if !state.qualifications.is_empty() || !state.pool.is_zero() {
            result.set_state(
                serde_json::to_value(&state).map_err(|e| CoreError::Validation(e.to_string()))?,
            );
        }

        // Quietly consume FlushlineAccount for module linkage symmetry.
        let _ = std::marker::PhantomData::<RoyalFlushlineAccount>;

        Ok(result)
    }
}

fn bump_qual(state: &mut PotBonusState, user: UserId, graduation: bool, cycle: bool) {
    if let Some(q) = state.qualifications.iter_mut().find(|q| q.user_id == user) {
        if graduation {
            q.record_graduation();
        }
        if cycle {
            q.record_matrix_cycle();
        }
    } else {
        let mut q = RoyalQualification::new(user);
        if graduation {
            q.record_graduation();
        }
        if cycle {
            q.record_matrix_cycle();
        }
        state.qualifications.push(q);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payplan::events::DomainEvent;
    use crate::shared::ids::{CompanyId, EventId, PackageId};

    fn qualified(user: UserId) -> RoyalQualification {
        let mut q = RoyalQualification::new(user);
        q.record_graduation();
        q.record_matrix_cycle();
        assert!(q.is_qualified);
        q
    }

    /// Task 2: distributing the pot must produce ledger entries but must NOT
    /// re-emit `RoyalPotBonusDistributed` (which would self-cascade forever).
    /// It emits the terminal `RoyalPotBonusSettled` instead.
    #[test]
    fn distribution_emits_settled_not_distributed() {
        let module = RoyalPotBonusModule::new(RoyalPotBonusConfig::default());
        let company = CompanyId::new();
        let user = UserId::new();

        let state = PotBonusState {
            pool: Decimal::from(1000),
            qualifications: vec![qualified(user)],
        };

        let trigger = DomainEvent {
            id: EventId::new(),
            company_id: Some(company),
            event_type: EventType::RoyalPotBonusDistributed,
            payload: json!({}),
            created_at: Utc::now(),
        };

        let ctx = ModuleContext::new(company, PackageId::new())
            .with_module_state(serde_json::to_value(&state).unwrap())
            .with_event(trigger);

        let result = module.run(&ctx).expect("module run");

        // Ledger entries are produced for the qualified user.
        assert!(
            !result.ledger_entries.is_empty(),
            "expected ledger entries for the qualified user"
        );

        // No RoyalPotBonusDistributed is re-emitted (no self-cascade).
        assert!(
            result
                .emitted_events
                .iter()
                .all(|e| e.event_type != EventType::RoyalPotBonusDistributed),
            "handler must not re-emit its own trigger"
        );

        // Exactly one terminal settled event is emitted.
        let settled = result
            .emitted_events
            .iter()
            .filter(|e| e.event_type == EventType::RoyalPotBonusSettled)
            .count();
        assert_eq!(settled, 1, "expected exactly one RoyalPotBonusSettled");
    }
}
