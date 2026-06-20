//! Integration test for persistent module state.
//!
//! Verifies that a module's state survives across runs of the engine cascade
//! when the engine is given a `PgModuleStateStore`. Gated behind the
//! `integration` feature so the default `cargo test` doesn't need Postgres.
//!
//! Run with:
//!   `DATABASE_URL=postgres://... cargo test -p payplan_infra --features integration -- --include-ignored --test-threads=1`

#![cfg(feature = "integration")]

use std::sync::Arc;

use chrono::Utc;
use payplan_app::ports::{ModuleStateChange, ModuleStateStore};
use payplan_core::payplan::events::{DomainEvent, EventType};
use payplan_core::payplan::module::ModuleContext;
use payplan_core::payplan::registry::ModuleRegistry;
use payplan_core::payplan::runner::{StackRunner, StateCache};
use payplan_core::payplan::stack::{PayPlanStack, PayPlanStackStatus, StackModule};
use payplan_core::shared::ids::{CompanyId, EnrollmentId, PackageId, PayPlanStackId, UserId};
use payplan_infra::migrator;
use payplan_infra::module_state_store::PgModuleStateStore;
use payplan_infra::postgres::{connect, PgConfig};
use serde_json::json;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let cfg = PgConfig::from_env().expect("DATABASE_URL");
    let pool = connect(&cfg).await.expect("connect");
    migrator::run(&pool).await.expect("migrations");
    pool
}

async fn truncate_all(pool: &PgPool) {
    sqlx::query("TRUNCATE TABLE module_state RESTART IDENTITY")
        .execute(pool)
        .await
        .unwrap();
}

fn flushline_only_registry() -> Arc<ModuleRegistry> {
    let mut r = ModuleRegistry::new();
    r.register(
        payplan_core::modules::royal::flushline_module::RoyalFlushlineModule::new(
            Default::default(),
        ),
    );
    Arc::new(r)
}

fn build_stack(stack_id: PayPlanStackId, company_id: CompanyId) -> PayPlanStack {
    PayPlanStack {
        id: stack_id,
        company_id,
        name: "Flushline Test".into(),
        version: 1,
        status: PayPlanStackStatus::Active,
        modules: vec![StackModule {
            module_key: "royal.flushline".into(),
            module_version: "1.0.0".into(),
            sort_order: 10,
            config: json!({}),
            active: true,
        }],
        created_at: Utc::now(),
    }
}

fn package_purchased_event(
    company_id: CompanyId,
    user_id: UserId,
    package_id: PackageId,
    points: u32,
) -> DomainEvent {
    DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::PackagePurchased,
        payload: json!({
            "user_id": user_id,
            "package_id": package_id,
            "points": points,
            "volume": 0,
            "leg": "left",
        }),
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn flushline_state_persists_across_cascades() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let store = PgModuleStateStore::new(pool.clone());

    let company_id = CompanyId::new();
    let user_id = UserId::new();
    let package_id = PackageId::new();
    let stack_id = PayPlanStackId::new();
    let stack = build_stack(stack_id, company_id);
    let registry = flushline_only_registry();
    let runner = StackRunner::new((*registry).clone());

    // Simulate three "purchases" against the same enrollment. In real life
    // each purchase creates a new enrollment, but for the Flushline's
    // point-tracking the relevant unit is a stable aggregate. The test
    // verifies state persists across engine runs regardless of how the
    // engine is driven.
    let enrollment_id = EnrollmentId::new();
    let aggregate = enrollment_id.0;

    for _ in 1..=3 {
        // Load existing state.
        let mut conn = pool.acquire().await.expect("acquire");
        let existing = store
            .load_for_aggregate(aggregate, &mut conn)
            .await
            .expect("load");
        let mut cache = StateCache::new();
        for ((k, v), val) in &existing {
            cache.put(k, v, aggregate, val.clone());
        }
        drop(conn);

        // Run cascade.
        let event = package_purchased_event(company_id, user_id, package_id, 5);
        let ctx = ModuleContext::new(company_id, package_id)
            .with_enrollment(enrollment_id)
            .with_event(event.clone());
        let result = runner.run(&stack, &event, &ctx, &mut cache).expect("run");

        // Save state changes (only one module, one change).
        let mut conn = pool.acquire().await.expect("acquire");
        for change in result.state_changes {
            store
                .save(
                    ModuleStateChange {
                        module_key: &change.module_key,
                        module_version: &change.module_version,
                        aggregate_id: change.aggregate_id,
                        state: &change.value,
                    },
                    &mut conn,
                )
                .await
                .expect("save");
        }
    }

    // Verify final persisted state.
    let state: serde_json::Value = sqlx::query_scalar(
        "SELECT state FROM module_state WHERE module_key = 'royal.flushline' AND aggregate_id = $1",
    )
    .bind(aggregate)
    .fetch_one(&pool)
    .await
    .unwrap();
    let points = state
        .get("account")
        .and_then(|a| a.get("current_points"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    let graduated = state
        .get("account")
        .and_then(|a| a.get("graduated"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    assert_eq!(points, 15, "state persisted across 3 cascades: 5+5+5=15");
    assert!(graduated, "graduated at 15 cumulative points");
}

#[tokio::test]
async fn state_is_isolated_per_aggregate() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let store = PgModuleStateStore::new(pool.clone());

    let company_id = CompanyId::new();
    let user_id = UserId::new();
    let package_id = PackageId::new();
    let stack_id = PayPlanStackId::new();
    let stack = build_stack(stack_id, company_id);
    let registry = flushline_only_registry();
    let runner = StackRunner::new((*registry).clone());

    // Two different aggregates (enrollments). Each gets one 5-point grant.
    for aggregate_n in 0..2 {
        let aggregate = uuid::Uuid::now_v7();
        let mut conn = pool.acquire().await.expect("acquire");
        let existing = store
            .load_for_aggregate(aggregate, &mut conn)
            .await
            .expect("load");
        let mut cache = StateCache::new();
        for ((k, v), val) in &existing {
            cache.put(k, v, aggregate, val.clone());
        }
        drop(conn);
        let event = package_purchased_event(company_id, user_id, package_id, 5);
        let enrollment_id = EnrollmentId::new();
        let ctx = ModuleContext::new(company_id, package_id)
            .with_aggregate(aggregate)
            .with_enrollment(enrollment_id)
            .with_event(event.clone());
        let result = runner.run(&stack, &event, &ctx, &mut cache).expect("run");
        let mut conn = pool.acquire().await.expect("acquire");
        for change in result.state_changes {
            store
                .save(
                    ModuleStateChange {
                        module_key: &change.module_key,
                        module_version: &change.module_version,
                        aggregate_id: change.aggregate_id,
                        state: &change.value,
                    },
                    &mut conn,
                )
                .await
                .expect("save");
        }
        let _ = aggregate_n;
    }

    let rows = sqlx::query(
        "SELECT aggregate_id, state FROM module_state WHERE module_key = 'royal.flushline'",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2, "two independent aggregates persisted");
}
