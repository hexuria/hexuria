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
use payplan_core::shared::ids::{EnrollmentId, PackageId, PayPlanStackId, UserId};
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

fn build_stack(stack_id: PayPlanStackId) -> PayPlanStack {
    PayPlanStack {
        id: stack_id,
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
    user_id: UserId,
    package_id: PackageId,
    points: u32,
) -> DomainEvent {
    DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
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

    let user_id = UserId::new();
    let package_id = PackageId::new();
    let stack_id = PayPlanStackId::new();
    let stack = build_stack(stack_id);
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
        let event = package_purchased_event(user_id, package_id, 5);
        let ctx = ModuleContext::new(package_id)
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

fn binary_tree_registry() -> Arc<ModuleRegistry> {
    let mut r = ModuleRegistry::new();
    r.register(
        payplan_core::modules::binary::tree_module::BinaryTreeModule::new(Default::default()),
    );
    Arc::new(r)
}

fn binary_tree_stack(stack_id: PayPlanStackId) -> PayPlanStack {
    PayPlanStack {
        id: stack_id,
        name: "Binary Tree Test".into(),
        version: 1,
        status: PayPlanStackStatus::Active,
        modules: vec![StackModule {
            module_key: "binary.tree".into(),
            module_version: "1.0.0".into(),
            sort_order: 10,
            config: json!({}),
            active: true,
        }],
        created_at: Utc::now(),
    }
}

fn enrollment_created_event(
    user_id: UserId,
    package_id: PackageId,
    enrollment_id: EnrollmentId,
    sponsor: Option<UserId>,
) -> DomainEvent {
    DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
        event_type: EventType::EnrollmentCreated,
        payload: json!({
            "user_id": user_id,
            "package_id": package_id,
            "enrollment_id": enrollment_id,
            "sponsor_user_id": sponsor,
        }),
        created_at: Utc::now(),
    }
}

/// The binary tree is globally-scoped, so two enrollments form ONE shared tree —
/// the second is placed *under* the first, not as a second root. Each enrollment
/// is a separate engine run with its own cache; the globally-scoped state must
/// persist and reload between them.
#[tokio::test]
async fn binary_tree_is_globally_scoped_and_forms_one_tree() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let store = PgModuleStateStore::new(pool.clone());

    let package_id = PackageId::new();
    let stack_id = PayPlanStackId::new();
    let stack = binary_tree_stack(stack_id);
    let registry = binary_tree_registry();
    let runner = StackRunner::new((*registry).clone());

    let user_a = UserId::new();
    let user_b = UserId::new();
    let enroll_a = EnrollmentId::new();
    let enroll_b = EnrollmentId::new();

    // Two purchases: B is sponsored by A. globally-scoped tree state lives under
    // Uuid::nil() regardless of which enrollment triggered the placement.
    let purchases = [(user_a, enroll_a, None), (user_b, enroll_b, Some(user_a))];
    let global_aggregate = uuid::Uuid::nil();

    for (user_id, enrollment_id, sponsor) in purchases {
        // Reload state from BOTH namespaces, as the real driver does.
        let mut conn = pool.acquire().await.expect("acquire");
        let mut cache = StateCache::new();
        for ((k, v), val) in &store
            .load_for_aggregate(enrollment_id.0, &mut conn)
            .await
            .expect("load enrollment")
        {
            cache.put(k, v, enrollment_id.0, val.clone());
        }
        for ((k, v), val) in &store
            .load_for_aggregate(global_aggregate, &mut conn)
            .await
            .expect("load global")
        {
            cache.put(k, v, global_aggregate, val.clone());
        }
        drop(conn);

        let event =
            enrollment_created_event(user_id, package_id, enrollment_id, sponsor);
        let ctx = ModuleContext::new(package_id)
            .with_enrollment(enrollment_id)
            .with_event(event.clone());
        let result = runner.run(&stack, &event, &ctx, &mut cache).expect("run");

        let mut conn = pool.acquire().await.expect("acquire");
        for change in result.state_changes {
            assert_eq!(
                change.aggregate_id, global_aggregate,
                "binary.tree state must be keyed to the global Uuid::nil()"
            );
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

    // Exactly ONE globally-scoped tree row exists, with two nodes.
    let rows =
        sqlx::query("SELECT aggregate_id FROM module_state WHERE module_key = 'binary.tree'")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(rows.len(), 1, "one shared globally-scoped tree row");

    let state: serde_json::Value = sqlx::query_scalar(
        "SELECT state FROM module_state WHERE module_key = 'binary.tree' AND aggregate_id = $1",
    )
    .bind(global_aggregate)
    .fetch_one(&pool)
    .await
    .unwrap();

    let nodes = state.get("nodes").and_then(|n| n.as_array()).unwrap();
    assert_eq!(nodes.len(), 2, "both enrollments placed in the same tree");

    // First node is the root; second is placed under it (not a second root).
    let node_a = nodes
        .iter()
        .find(|n| n.get("user_id") == Some(&json!(user_a)))
        .expect("node a present");
    let node_b = nodes
        .iter()
        .find(|n| n.get("user_id") == Some(&json!(user_b)))
        .expect("node b present");
    assert!(
        node_a
            .get("parent_node_id")
            .map(|v| v.is_null())
            .unwrap_or(true),
        "first enrollment is the root"
    );
    assert_eq!(
        node_b.get("parent_node_id"),
        node_a.get("id"),
        "second enrollment is placed under the first, not as a second root"
    );
}

#[tokio::test]
async fn state_is_isolated_per_aggregate() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let store = PgModuleStateStore::new(pool.clone());

    let user_id = UserId::new();
    let package_id = PackageId::new();
    let stack_id = PayPlanStackId::new();
    let stack = build_stack(stack_id);
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
        let event = package_purchased_event(user_id, package_id, 5);
        let enrollment_id = EnrollmentId::new();
        let ctx = ModuleContext::new(package_id)
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
