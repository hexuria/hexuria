//! Scheduled workflows (PRD §12 Phase 7).
//!
//! These jobs run on a schedule (cron in production, manual trigger in dev):
//!
//! - [`run_renewals`]: process subscriptions whose grace period has elapsed.
//! - [`run_royal_pot_distribution`]: trigger weekly Royal pot bonus distribution.
//! - [`close_binary_cycles`]: close open binary cycle periods and emit `BinaryCycleClosed`.
//!
//! Each job is a pure function over its inputs; the calling layer (Spin
//! scheduler in production, axum handler in dev) is responsible for the loop
//! and concurrency control.

use chrono::{DateTime, Utc};
use payplan_app::commands::PurchaseDeps;
use payplan_app::error::AppResult;
use payplan_core::payplan::events::{DomainEvent, EventType};
use payplan_core::payplan::module::ModuleContext;
use payplan_core::payplan::registry::ModuleRegistry;
use payplan_core::payplan::runner::{StackRunResult, StackRunner, StateCache};
use payplan_core::shared::ids::{EnrollmentId, PackageId, PayPlanStackId};
use serde_json::json;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{info, instrument};
use uuid::Uuid;

/// Run all due renewals. For each subscription whose `current_period_end` has
/// elapsed, emits a `SubscriptionRenewed` event and re-runs the package's pay
/// plan stack so modules like binary.volume can pick up new commissionable
/// volume from the renewal.
#[instrument(skip(pool, deps))]
pub async fn run_renewals(pool: &PgPool, deps: &PurchaseDeps<'_>) -> AppResult<usize> {
    let now = Utc::now();
    // Select due recurring subscriptions along with their billing plan's
    // recurrence_interval so we can compute the next period end correctly.
    // `billing_type = 'recurring'` excludes one-time billing plans.
    let due: Vec<(Uuid, Uuid, Uuid, Uuid, Option<String>)> = sqlx::query_as(
        r#"SELECT s.id, s.user_id, s.package_id, p.company_id, bp.recurrence_interval
           FROM subscriptions s
           JOIN packages p ON p.id = s.package_id
           JOIN billing_plans bp ON bp.id = s.billing_plan_id
           WHERE s.status = 'active'
             AND bp.billing_type = 'recurring'
             AND s.current_period_end IS NOT NULL
             AND s.current_period_end < $1"#,
    )
    .bind(now)
    .fetch_all(pool)
    .await
    .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;

    let count = due.len();
    info!(due = count, "processing subscription renewals");

    let registry: Arc<ModuleRegistry> = deps.registry.clone();
    let runner = StackRunner::new((*registry).clone());

    for (sub_id, user_id, package_id, company_id, recurrence_interval) in due {
        // Load package's commissionable volume + points.
        let (comm_volume, points) = load_package_renewal_shape(pool, package_id).await?;

        // Resolve the binary node for this subscription's user so the volume
        // module can credit the right node (and so we can look up the last
        // leg used for THAT node — leg alternation must be per-node, not
        // per-package, otherwise two users on the same package cross-pollute).
        let node_id: Option<Uuid> = sqlx::query_scalar(
            r#"SELECT bn.id FROM binary_nodes bn
               JOIN enrollments e ON e.id = bn.enrollment_id
               WHERE e.user_id = $1 AND e.company_id = $2 AND e.status = 'active'
               LIMIT 1"#,
        )
        .bind(user_id)
        .bind(company_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| payplan_app::error::AppError::Infra(format!("lookup renewal node: {e}")))?
        .flatten();

        let last_leg: Option<String> = match node_id {
            Some(nid) => sqlx::query_scalar(
                r#"SELECT leg FROM binary_volume_ledger
                   WHERE node_id = $1 ORDER BY created_at DESC LIMIT 1"#,
            )
            .bind(nid)
            .fetch_optional(pool)
            .await
            .map_err(|e| payplan_app::error::AppError::Infra(format!("lookup last leg: {e}")))?
            .flatten(),
            None => {
                tracing::warn!(
                    user_id = %user_id,
                    "run_renewals: no binary_node for user, volume will skip projection"
                );
                None
            }
        };
        let leg = match last_leg.as_deref() {
            Some("left") => "right",
            _ => "left",
        };

        let mut payload = json!({
            "subscription_id": sub_id,
            "user_id": user_id,
            "package_id": package_id,
            "volume": comm_volume,
            "points": points,
            "leg": leg,
        });
        if let Some(nid) = node_id {
            payload
                .as_object_mut()
                .unwrap()
                .insert("node_id".into(), json!(nid));
        }

        let event = DomainEvent {
            id: payplan_core::shared::ids::EventId::new(),
            company_id: Some(payplan_core::shared::ids::CompanyId::from(company_id)),
            event_type: EventType::SubscriptionRenewed,
            payload,
            created_at: now,
        };
        run_stack_against_event(pool, deps, &runner, &event).await?;

        // Bump the period using the billing plan's recurrence interval
        // (monthly/weekly/quarterly/annual; unknown falls back to monthly).
        // Uses make_interval() with bound parameters instead of format!-built SQL.
        let (months, days): (i32, i32) = match recurrence_interval.as_deref() {
            Some("weekly") => (0, 7),
            Some("quarterly") => (3, 0),
            Some("annual") | Some("yearly") => (12, 0),
            _ => (1, 0),
        };
        sqlx::query(
            r#"UPDATE subscriptions
               SET current_period_start = $1,
                   current_period_end = $1 + make_interval(months => $3, days => $4)
               WHERE id = $2"#,
        )
        .bind(now)
        .bind(sub_id)
        .bind(months)
        .bind(days)
        .execute(pool)
        .await
        .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;
    }

    Ok(count)
}

/// Load the renewal shape (commissionable volume, points) from the package's
/// items. Honors `is_commissionable` and multiplies by `quantity` to match the
/// domain semantics. Returns `(total_volume, total_points)`.
async fn load_package_renewal_shape(pool: &PgPool, package_id: Uuid) -> AppResult<(i64, u32)> {
    let row: Option<(Option<i64>, Option<i32>)> = sqlx::query_as(
        r#"SELECT
              COALESCE(SUM(pi.commissionable_volume * pi.quantity)
                       FILTER (WHERE pi.is_commissionable), 0)::BIGINT AS volume,
              COALESCE(SUM(pi.points_value * pi.quantity)
                       FILTER (WHERE pi.is_commissionable), 0)::INT AS points
           FROM package_items pi
           WHERE pi.package_id = $1"#,
    )
    .bind(package_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;
    let (volume, points) = row.unwrap_or((None, None));
    Ok((volume.unwrap_or(0), points.unwrap_or(0).max(0) as u32))
}

/// Trigger a Royal Flush pot distribution by emitting `RoyalPotBonusDistributed`
/// against every company with at least one qualified user.
#[instrument(skip(pool, deps))]
pub async fn run_royal_pot_distribution(
    pool: &PgPool,
    deps: &PurchaseDeps<'_>,
) -> AppResult<usize> {
    let registry = deps.registry.clone();
    let runner = StackRunner::new((*registry).clone());

    let companies: Vec<Uuid> = sqlx::query_scalar(
        r#"SELECT DISTINCT company_id FROM royal_qualifications WHERE is_qualified = TRUE"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;
    info!(
        companies = companies.len(),
        "triggering Royal pot distribution"
    );

    let processed = companies.len();
    for company_id in &companies {
        let mut pool_conn = pool
            .acquire()
            .await
            .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;
        let conn: &mut sqlx::PgConnection = pool_conn.as_mut();
        let event = DomainEvent {
            id: payplan_core::shared::ids::EventId::new(),
            company_id: Some(payplan_core::shared::ids::CompanyId::from(*company_id)),
            event_type: EventType::RoyalPotBonusDistributed,
            payload: json!({}),
            created_at: Utc::now(),
        };
        // The pot-distribution event is company-wide and carries no
        // `package_id`, so `run_stack_against_event` short-circuits at its
        // `package_id` guard without persisting anything. We therefore persist
        // the trigger here. (No duplicate results: the early return means the
        // function never reaches its own append.) NOTE: because of that early
        // return the pot-bonus cascade does not actually run through this job
        // path — see REMEDIATION_PLAN Task 5 (aggregate scoping) for the deeper
        // fix that makes company-scoped distribution cascade properly.
        deps.events
            .append(std::slice::from_ref(&event), &mut *conn)
            .await?;
        run_stack_against_event(pool, deps, &runner, &event).await?;
    }

    Ok(processed)
}

/// Close all open `binary_cycle_periods` for each company and emit
/// `BinaryCycleClosed` against the package's pay plan stack so pairing and
/// carryover modules can settle.
#[instrument(skip(pool, deps))]
pub async fn close_binary_cycles(pool: &PgPool, deps: &PurchaseDeps<'_>) -> AppResult<usize> {
    let registry = deps.registry.clone();
    let runner = StackRunner::new((*registry).clone());

    let open: Vec<(Uuid, Uuid)> =
        sqlx::query_as(r#"SELECT id, company_id FROM binary_cycle_periods WHERE status = 'open'"#)
            .fetch_all(pool)
            .await
            .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;

    let count = open.len();
    info!(open = count, "closing binary cycle periods");

    for (period_id, company_id) in open {
        let enrollments: Vec<(Uuid, Uuid, Uuid)> = sqlx::query_as(
            r#"SELECT id, package_id, user_id FROM enrollments
               WHERE company_id = $1 AND status = 'active'"#,
        )
        .bind(company_id)
        .fetch_all(pool)
        .await
        .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;

        for (enrollment_id, package_id, user_id) in enrollments {
            // Resolve the binary node for this enrollment so downstream
            // modules (pairing, carryover) and the event projector can use
            // node_id directly instead of looking it up.
            let node_id: Option<Uuid> =
                sqlx::query_scalar(r#"SELECT id FROM binary_nodes WHERE enrollment_id = $1"#)
                    .bind(enrollment_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| {
                        payplan_app::error::AppError::Infra(format!("lookup binary_node: {e}"))
                    })?
                    .flatten();

            let mut payload = json!({
                "period_id": period_id,
                "enrollment_id": enrollment_id,
                "node_user_id": user_id,
                "package_id": package_id,
            });
            if let Some(nid) = node_id {
                payload
                    .as_object_mut()
                    .unwrap()
                    .insert("node_id".into(), json!(nid));
            } else {
                tracing::warn!(
                    enrollment_id = %enrollment_id,
                    "close_binary_cycles: no binary_node found for enrollment, node_id omitted"
                );
            }

            let event = DomainEvent {
                id: payplan_core::shared::ids::EventId::new(),
                company_id: Some(payplan_core::shared::ids::CompanyId::from(company_id)),
                event_type: EventType::BinaryCycleClosed,
                payload,
                created_at: Utc::now(),
            };
            run_stack_against_event(pool, deps, &runner, &event).await?;
        }

        sqlx::query(
            r#"UPDATE binary_cycle_periods SET status = 'closed', closed_at = $1 WHERE id = $2"#,
        )
        .bind(Utc::now())
        .bind(period_id)
        .execute(pool)
        .await
        .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;
    }

    Ok(count)
}

/// Run a single event through the package's pay plan stack (best-effort).
async fn run_stack_against_event(
    pool: &PgPool,
    deps: &PurchaseDeps<'_>,
    runner: &StackRunner,
    event: &DomainEvent,
) -> AppResult<()> {
    let Some(company_id) = event.company_id else {
        return Ok(());
    };
    let Some(payload_package_id) = event.payload.get("package_id").and_then(|v| v.as_str()) else {
        return Ok(());
    };
    let package_uuid = Uuid::parse_str(payload_package_id).map_err(|e| {
        payplan_app::error::AppError::Validation(format!("invalid package_id: {e}"))
    })?;
    let package_id = PackageId::from(package_uuid);

    let mut pool_conn = pool
        .acquire()
        .await
        .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;
    let conn: &mut sqlx::PgConnection = pool_conn.as_mut();

    let stack_id: Option<Uuid> =
        sqlx::query_scalar(r#"SELECT pay_plan_stack_id FROM packages WHERE id = $1"#)
            .bind(package_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?
            .flatten();

    let Some(stack_id) = stack_id else {
        // Stack-less package: there is no cascade, so just persist the trigger
        // (a single INSERT — atomic on its own) and return.
        deps.events
            .append(std::slice::from_ref(event), &mut *conn)
            .await?;
        return Ok(());
    };

    let stack = deps
        .pay_plan_stacks
        .get(PayPlanStackId::from(stack_id), &mut *conn)
        .await?
        .ok_or_else(|| {
            payplan_app::error::AppError::NotFound(format!("stack {stack_id} not found"))
        })?;

    let enrollment_id = event
        .payload
        .get("enrollment_id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .map(EnrollmentId::from);

    let ctx = ModuleContext::new(company_id, package_id);
    let ctx = match enrollment_id {
        Some(eid) => ctx.with_enrollment(eid),
        None => ctx,
    };
    let ctx = ctx.with_event(event.clone());

    let mut cache = StateCache::new();
    // Pre-load existing module state. Enrollment-scoped modules key state under
    // the enrollment id (when known); company-scoped modules (binary tree/
    // carryover, royal pot) key under the company id. We seed both namespaces;
    // the runner picks the right one per module via `Module::scope()`.
    if let Some(store) = deps.module_state_store {
        if let Some(eid) = enrollment_id {
            let by_enrollment = store.load_for_aggregate(eid.0, &mut *conn).await?;
            for ((module_key, module_version), value) in &by_enrollment {
                cache.put(module_key, module_version, eid.0, value.clone());
            }
        }
        let by_company = store.load_for_aggregate(company_id.0, &mut *conn).await?;
        for ((module_key, module_version), value) in &by_company {
            cache.put(module_key, module_version, company_id.0, value.clone());
        }
    }

    // `emitted[0]` is the triggering event; the cascade drives module runs off
    // it and appends any newly emitted events. Unlike the previous code, the
    // trigger is NOT persisted before this point — the entire `emitted` vec
    // (trigger included) is appended exactly once inside the transaction below,
    // so there is no duplicate-key risk (Task 1) and the whole write is atomic
    // (Task 10).
    let mut emitted = vec![event.clone()];
    let mut ledger = vec![];
    let mut state_changes: Vec<payplan_core::payplan::runner::StateChange> = vec![];
    let mut processed = 0;
    // Hard cap to prevent a misbehaving/self-emitting module from looping
    // forever (Path A has the same guard in commands.rs::run_stack).
    const MAX_ITERATIONS: usize = 32;
    let mut iterations = 0;
    while processed < emitted.len() {
        if iterations >= MAX_ITERATIONS {
            return Err(payplan_app::error::AppError::Conflict(format!(
                "cascade exceeded {MAX_ITERATIONS} iterations; aborting"
            )));
        }
        iterations += 1;
        let ev = emitted[processed].clone();
        processed += 1;
        // Inherit the outer context (company + enrollment + package) and only
        // swap the triggering event, so enrollment-scoped cascade modules keep
        // their aggregate id. All cascade events share this run's company.
        let ctx2 = ctx.clone().with_event(ev.clone());
        let result: AppResult<StackRunResult> = runner
            .run(&stack, &ev, &ctx2, &mut cache)
            .map_err(payplan_app::error::AppError::from);
        let result = result?;
        emitted.extend(result.emitted_events);
        ledger.extend(result.ledger_entries);
        state_changes.extend(result.state_changes);
    }

    // All reads are done; release the pooled read connection before opening the
    // write transaction so we never hold two connections from the pool at once.
    drop(pool_conn);

    // Atomic write+project (Task 10): event append, ledger append,
    // module_state save, and BOTH projectors run inside ONE transaction. A
    // failure anywhere rolls the whole thing back — no more drift where
    // module_state is written without its projection row. Mirrors
    // PgPurchaseWriter::write (Path A).
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| payplan_app::error::AppError::Infra(e.to_string()))?;

    // Append the full `emitted` vec (trigger at [0] + cascade output) exactly
    // once.
    deps.events.append(&emitted, &mut tx).await?;
    if !ledger.is_empty() {
        deps.ledger.append(&ledger, &mut tx).await?;
    }
    if let Some(store) = deps.module_state_store {
        for change in &state_changes {
            store
                .save(
                    payplan_app::ports::ModuleStateChange {
                        module_key: &change.module_key,
                        module_version: &change.module_version,
                        aggregate_id: change.aggregate_id,
                        state: &change.value,
                    },
                    &mut tx,
                )
                .await?;
        }
    }
    // Per-module projections (Track A2-A5).
    if let Some(projector) = deps.projector {
        projector.project(&state_changes, &mut tx).await?;
    }
    // Event-driven projections (Track B1/B2): materialise rows from emitted
    // events.
    if let Some(event_projector) = deps.event_projector {
        event_projector.project(&emitted, &mut tx).await?;
    }

    tx.commit()
        .await
        .map_err(|e| payplan_app::error::AppError::Infra(format!("commit: {e}")))?;

    Ok(())
}

#[allow(dead_code)]
const _NOW_TYPE: Option<DateTime<Utc>> = None;
