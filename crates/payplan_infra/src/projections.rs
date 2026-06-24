//! Projects `module_state` JSON changes into the per-module relational tables.
//!
//! Invoked by `PgPurchaseWriter` (inside the atomic purchase transaction) and by
//! `operations::run_stack_against_event` (Path B scheduled jobs). For each
//! `StateChange` the projector decodes the module's typed state struct from the
//! JSON blob and runs idempotent upserts into the matching table:
//!
//! - `royal.flushline`  -> `royal_flushline_accounts`
//! - `binary.tree`      -> `binary_nodes`
//! - `binary.volume`    -> `binary_volume_ledger`
//! - `binary.carryover` -> `binary_carryover`
//!
//! All upserts execute against the caller's `&mut PgConnection`, so in Path A
//! they join the single atomic purchase transaction. The projector performs no
//! DB lookups: every key the target table needs is carried in the augmented
//! state struct. Rows that lack a resolvable key
//! (e.g. `binary_volume_ledger` entries without `node_id`) are skipped with a
//! `tracing::warn!` rather than failing the cascade — their full linkage lands
//! with Track B (cycle-close, renewal).
//!
//! The [`PgEventProjector`] below handles Track B1/B2: materialising rows from
//! emitted domain events that can't be derived from module state (e.g. a
//! `RoyalAccountDuplicated` event creates a new enrollment + flushline account).

use async_trait::async_trait;
use payplan_app::error::{AppError, AppResult};
use payplan_app::ports::{EventProjector, ModuleProjector};
use payplan_core::modules::binary::carryover_module::CarryoverState;
use payplan_core::modules::binary::tree::BinaryLeg;
use payplan_core::modules::binary::tree_module::BinaryTreeState;
use payplan_core::modules::binary::volume::BinaryVolumeStatus;
use payplan_core::modules::binary::volume_module::BinaryVolumeState;
use payplan_core::modules::royal::flushline_module::FlushlineState;
use payplan_core::payplan::events::{DomainEvent, EventType};
use payplan_core::payplan::runner::StateChange;
use sqlx::PgConnection;
use tracing::warn;

/// Postgres-backed implementation of [`ModuleProjector`]. Stateless: it
/// receives `&mut PgConnection` from the caller's transaction.
pub struct PgProjections;

impl PgProjections {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for PgProjections {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ModuleProjector for PgProjections {
    async fn project(&self, changes: &[StateChange], conn: &mut PgConnection) -> AppResult<()> {
        for change in changes {
            match change.module_key.as_str() {
                "royal.flushline" => project_flushline(change, conn).await?,
                "binary.tree" => project_binary_nodes(change, conn).await?,
                "binary.volume" => project_binary_volume(change, conn).await?,
                "binary.carryover" => project_binary_carryover(change, conn).await?,
                _ => {}
            }
        }
        Ok(())
    }
}

/// Decode a `StateChange` blob into a typed state struct. Mismatches produce an
/// `AppError::Infra` (rather than being silently skipped) so a module shipping
/// a state shape the projector can't read surfaces loudly.
fn decode<T: serde::de::DeserializeOwned>(change: &StateChange) -> AppResult<T> {
    serde_json::from_value::<T>(change.value.clone())
        .map_err(|e| AppError::Infra(format!("project {}: decode state: {e}", change.module_key)))
}

/// `royal.flushline` -> `royal_flushline_accounts`. Clean 1:1: the account
/// struct already carries every column the table needs.
async fn project_flushline(change: &StateChange, conn: &mut PgConnection) -> AppResult<()> {
    let state: FlushlineState = decode(change)?;
    let Some(account) = state.account else {
        return Ok(());
    };

    let tier = serde_json::to_value(account.current_tier)
        .map_err(|e| AppError::Infra(format!("encode tier: {e}")))?;
    let tier_str = tier
        .as_str()
        .ok_or_else(|| AppError::Infra("tier did not serialize to a string".into()))?
        .to_string();

    sqlx::query(
        r#"INSERT INTO royal_flushline_accounts
           (id, enrollment_id, owner_user_id, current_tier, current_points,
            cycle_count, graduated, graduated_at, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           ON CONFLICT (id) DO UPDATE SET
             current_tier    = EXCLUDED.current_tier,
             current_points  = EXCLUDED.current_points,
             graduated       = EXCLUDED.graduated,
             graduated_at    = EXCLUDED.graduated_at"#,
    )
    .bind(account.id)
    .bind(account.enrollment_id)
    .bind(account.owner_user_id)
    .bind(&tier_str)
    .bind(i32::try_from(account.current_points).unwrap_or(i32::MAX))
    .bind(0_i32)
    .bind(account.graduated)
    .bind(account.graduated_at)
    .bind(account.created_at)
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Infra(format!("upsert royal_flushline_accounts: {e}")))?;
    Ok(())
}

/// `binary.tree` -> `binary_nodes` (one upsert per node in the tree state).
async fn project_binary_nodes(change: &StateChange, conn: &mut PgConnection) -> AppResult<()> {
    let state: BinaryTreeState = decode(change)?;
    for node in &state.nodes {
        let Some(enrollment_id) = node.enrollment_id else {
            warn!(node_id = %node.id, "binary.tree: node missing enrollment_id, skipping projection");
            continue;
        };
        let leg = node.leg.map(leg_str);
        sqlx::query(
            r#"INSERT INTO binary_nodes
               (id, enrollment_id, user_id, sponsor_user_id, parent_node_id, leg)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO UPDATE SET
                 enrollment_id   = EXCLUDED.enrollment_id,
                 user_id         = EXCLUDED.user_id,
                 sponsor_user_id = EXCLUDED.sponsor_user_id,
                 parent_node_id  = EXCLUDED.parent_node_id,
                 leg             = EXCLUDED.leg"#,
        )
        .bind(node.id)
        .bind(enrollment_id)
        .bind(node.user_id)
        .bind(node.sponsor_user_id)
        .bind(node.parent_node_id)
        .bind(leg)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(format!("upsert binary_nodes: {e}")))?;
    }
    Ok(())
}

/// `binary.volume` -> `binary_volume_ledger` (append-only by entry id). Entries
/// without a resolved `node_id` are skipped until Track B links them.
async fn project_binary_volume(change: &StateChange, conn: &mut PgConnection) -> AppResult<()> {
    let state: BinaryVolumeState = decode(change)?;
    for entry in &state.entries {
        let Some(node_id) = entry.node_id else {
            warn!(
                entry_id = %entry.id,
                "binary.volume: entry missing node_id, skipping projection (Track B will link)"
            );
            continue;
        };
        let leg = leg_str(entry.leg);
        let status = volume_status_str(entry.status);
        sqlx::query(
            r#"INSERT INTO binary_volume_ledger
               (id, node_id, source_purchase_id, leg, volume, status)
               VALUES ($1, $2, $3, $4, $5, $6)
               ON CONFLICT (id) DO UPDATE SET
                 node_id           = EXCLUDED.node_id,
                 source_purchase_id = EXCLUDED.source_purchase_id,
                 leg               = EXCLUDED.leg,
                 volume            = EXCLUDED.volume,
                 status            = EXCLUDED.status"#,
        )
        .bind(entry.id)
        .bind(node_id)
        .bind(entry.source_purchase_id)
        .bind(leg)
        .bind(entry.volume)
        .bind(status)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(format!("upsert binary_volume_ledger: {e}")))?;
    }
    Ok(())
}

/// `binary.carryover` -> `binary_carryover` (PK node_id).
async fn project_binary_carryover(change: &StateChange, conn: &mut PgConnection) -> AppResult<()> {
    let state: CarryoverState = decode(change)?;
    let Some(node_id) = state.carry.node_id else {
        warn!("binary.carryover: missing node_id, skipping projection (Track B will link)");
        return Ok(());
    };
    sqlx::query(
        r#"INSERT INTO binary_carryover
           (node_id, left_carryover, right_carryover)
           VALUES ($1, $2, $3)
           ON CONFLICT (node_id) DO UPDATE SET
             left_carryover  = EXCLUDED.left_carryover,
             right_carryover = EXCLUDED.right_carryover"#,
    )
    .bind(node_id)
    .bind(state.carry.left_volume)
    .bind(state.carry.right_volume)
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Infra(format!("upsert binary_carryover: {e}")))?;
    Ok(())
}

fn leg_str(leg: BinaryLeg) -> &'static str {
    match leg {
        BinaryLeg::Left => "left",
        BinaryLeg::Right => "right",
    }
}

fn volume_status_str(status: BinaryVolumeStatus) -> &'static str {
    match status {
        BinaryVolumeStatus::Open => "open",
        BinaryVolumeStatus::Matched => "matched",
        BinaryVolumeStatus::Carried => "carried",
    }
}

// ===========================================================================
// Event-driven projections (Track B1 + B2)
// ===========================================================================

/// Postgres-backed implementation of [`EventProjector`]. Stateless.
pub struct PgEventProjector;

impl PgEventProjector {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for PgEventProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EventProjector for PgEventProjector {
    async fn project(&self, events: &[DomainEvent], conn: &mut PgConnection) -> AppResult<()> {
        for event in events {
            match event.event_type {
                EventType::RoyalAccountDuplicated => project_duplication(event, conn).await?,
                EventType::BinaryPairMatched => project_pairing_result(event, events, conn).await?,
                EventType::BinaryCycleClosed => advance_cycle_count(event, conn).await?,
                EventType::RoyalPotBonusSettled => project_pot_bonus_balances(event, conn).await?,
                _ => {}
            }
        }
        Ok(())
    }
}

fn uuid_field(payload: &serde_json::Value, key: &str) -> Option<uuid::Uuid> {
    payload
        .get(key)
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
}

fn i64_field(payload: &serde_json::Value, key: &str) -> Option<i64> {
    payload.get(key).and_then(|v| v.as_i64())
}

/// B1: On `RoyalAccountDuplicated`, create a new enrollment and seed a new
/// `royal_flushline_accounts` row at the default tier (`Ten`, 0 points).
async fn project_duplication(event: &DomainEvent, conn: &mut PgConnection) -> AppResult<()> {
    let Some(owner_user_id) = uuid_field(&event.payload, "owner_user_id") else {
        warn!("RoyalAccountDuplicated: missing owner_user_id, skipping");
        return Ok(());
    };
    let Some(new_royal_account_id) = uuid_field(&event.payload, "new_royal_account_id") else {
        warn!("RoyalAccountDuplicated: missing new_royal_account_id, skipping");
        return Ok(());
    };
    let package_id = uuid_field(&event.payload, "package_id");

    // Create a new enrollment for the duplicated account. The schema
    // requires a non-null purchase_id FK, but duplication is not itself a
    // purchase; insert a zero-amount placeholder purchase so the FK holds.
    let new_enrollment_id = uuid::Uuid::now_v7();
    let placeholder_purchase_id = uuid::Uuid::now_v7();
    sqlx::query(
        r#"INSERT INTO purchases
           (id, user_id, package_id, gross_amount, net_amount, currency, status, purchased_at)
           VALUES ($1, $2, $3, 0, 0, 'USD', 'paid', NOW())"#,
    )
    .bind(placeholder_purchase_id)
    .bind(owner_user_id)
    .bind(package_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Infra(format!("insert placeholder purchase for duplication: {e}")))?;

    sqlx::query(
        r#"INSERT INTO enrollments
           (id, user_id, package_id, purchase_id, status, joined_at)
           VALUES ($1, $2, $3, $4, 'active', NOW())"#,
    )
    .bind(new_enrollment_id)
    .bind(owner_user_id)
    .bind(package_id)
    .bind(placeholder_purchase_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Infra(format!("insert duplicated enrollment: {e}")))?;

    // Seed the new royal_flushline_accounts row at the default tier.
    sqlx::query(
        r#"INSERT INTO royal_flushline_accounts
           (id, enrollment_id, owner_user_id, current_tier, current_points,
            cycle_count, graduated, graduated_at, created_at)
           VALUES ($1, $2, $3, 'Ten', 0, 0, FALSE, NULL, NOW())"#,
    )
    .bind(new_royal_account_id)
    .bind(new_enrollment_id)
    .bind(owner_user_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Infra(format!("insert duplicated flushline account: {e}")))?;

    Ok(())
}

/// B2: On `BinaryPairMatched`, insert a `binary_pairing_results` row. The
/// commission points are recovered from a companion `BinaryCommissionEarned`
/// event in the same batch (matched by node_user_id); if absent, the row is
/// recorded with points = 0.
async fn project_pairing_result(
    event: &DomainEvent,
    all_events: &[DomainEvent],
    conn: &mut PgConnection,
) -> AppResult<()> {
    let Some(node_user_id) = uuid_field(&event.payload, "node_user_id") else {
        warn!("BinaryPairMatched: missing node_user_id, skipping");
        return Ok(());
    };
    let Some(node_id) = uuid_field(&event.payload, "node_id") else {
        warn!("BinaryPairMatched: missing node_id, skipping");
        return Ok(());
    };
    let Some(period_id) = uuid_field(&event.payload, "period_id") else {
        warn!("BinaryPairMatched: missing period_id, skipping");
        return Ok(());
    };
    let left = i64_field(&event.payload, "left").unwrap_or(0);
    let right = i64_field(&event.payload, "right").unwrap_or(0);
    let matched = i64_field(&event.payload, "matched").unwrap_or(0);

    // Recover the companion commission event + ledger entry id (if any).
    let commission_event = all_events.iter().find(|e| {
        e.event_type == EventType::BinaryCommissionEarned
            && uuid_field(&e.payload, "node_user_id") == Some(node_user_id)
    });
    let points = match commission_event {
        Some(ce) => i64_field(&ce.payload, "points").unwrap_or(0),
        None => 0,
    };
    let ledger_entry_id: Option<uuid::Uuid> = None;

    sqlx::query(
        r#"INSERT INTO binary_pairing_results
           (id, period_id, user_id, node_id, left_volume, right_volume,
            matched_volume, points, ledger_entry_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
    )
    .bind(uuid::Uuid::now_v7())
    .bind(period_id)
    .bind(node_user_id)
    .bind(node_id)
    .bind(left)
    .bind(right)
    .bind(matched)
    .bind(points)
    .bind(ledger_entry_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| AppError::Infra(format!("insert binary_pairing_results: {e}")))?;

    Ok(())
}

/// B2: On `BinaryCycleClosed`, advance `binary_nodes.cycle_count` for the node
/// referenced by the event payload.
async fn advance_cycle_count(event: &DomainEvent, conn: &mut PgConnection) -> AppResult<()> {
    let node_id = uuid_field(&event.payload, "node_id");
    let node_user_id = uuid_field(&event.payload, "node_user_id");

    let affected = if let Some(nid) = node_id {
        sqlx::query(
            r#"UPDATE binary_nodes SET cycle_count = cycle_count + 1
               WHERE id = $1"#,
        )
        .bind(nid)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(format!("advance cycle_count by node_id: {e}")))?
        .rows_affected()
    } else if let Some(uid) = node_user_id {
        sqlx::query(
            r#"UPDATE binary_nodes SET cycle_count = cycle_count + 1
               WHERE user_id = $1"#,
        )
        .bind(uid)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(format!("advance cycle_count by user_id: {e}")))?
        .rows_affected()
    } else {
        warn!("BinaryCycleClosed: missing node_id and node_user_id, skipping cycle_count advance");
        return Ok(());
    };

    if affected == 0 {
        warn!(
            node_id = ?node_id,
            "BinaryCycleClosed: no binary_node matched, cycle_count not advanced"
        );
    }
    Ok(())
}

/// B4: On `RoyalPotBonusSettled`, upsert per-user cumulative balances from
/// the `distributions` array the pot bonus module adds to the event payload.
async fn project_pot_bonus_balances(event: &DomainEvent, conn: &mut PgConnection) -> AppResult<()> {
    let Some(distributions) = event
        .payload
        .get("distributions")
        .and_then(|v| v.as_array())
    else {
        return Ok(());
    };
    if distributions.is_empty() {
        return Ok(());
    }

    for dist in distributions {
        let Some(user_id) = uuid_field(dist, "user_id") else {
            warn!("RoyalPotBonusSettled: distribution entry missing user_id, skipping entry");
            continue;
        };
        let profit_share = i64_field(dist, "profit_share").unwrap_or(0).max(0);
        let top_cycler = i64_field(dist, "top_cycler").unwrap_or(0).max(0);
        let total = profit_share + top_cycler;

        sqlx::query(
            r#"INSERT INTO royal_pot_bonus_balances
               (user_id, total_earned, profit_share_earned,
                top_cycler_earned, distributions_count, last_distribution_at)
               VALUES ($1, $2, $3, $4, 1, NOW())
               ON CONFLICT (user_id) DO UPDATE SET
                 total_earned        = royal_pot_bonus_balances.total_earned        + EXCLUDED.total_earned,
                 profit_share_earned = royal_pot_bonus_balances.profit_share_earned + EXCLUDED.profit_share_earned,
                 top_cycler_earned   = royal_pot_bonus_balances.top_cycler_earned   + EXCLUDED.top_cycler_earned,
                 distributions_count = royal_pot_bonus_balances.distributions_count + 1,
                 last_distribution_at = NOW()"#,
        )
        .bind(user_id)
        .bind(total)
        .bind(profit_share)
        .bind(top_cycler)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(format!("upsert royal_pot_bonus_balances: {e}")))?;
    }
    Ok(())
}
