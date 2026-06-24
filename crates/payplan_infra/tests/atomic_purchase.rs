//! Integration tests for the atomic `PgPurchaseWriter`.
//!
//! Gated behind the `integration` feature so the default `cargo test` doesn't
//! need a Postgres. Run with:
//!   `DATABASE_URL=postgres://... cargo test -p payplan_infra --features integration -- --include-ignored`

#![cfg(feature = "integration")]

use chrono::Utc;
use payplan_app::error::AppResult;
use payplan_app::ports::{PurchaseWriter, PurchaseWrites};
use payplan_core::payplan::events::{DomainEvent, EventType};
use payplan_core::payplan::ledger::{LedgerStatus, RewardLedgerEntry};
use payplan_core::platform::enrollment::{Enrollment, EnrollmentStatus};
use payplan_core::platform::entitlement::{Entitlement, EntitlementStatus};
use payplan_core::platform::purchase::{Purchase, PurchaseStatus};
use payplan_core::platform::subscription::{Subscription, SubscriptionStatus};
use payplan_core::shared::ids::{
    EnrollmentId, LedgerEntryId, PackageId, PurchaseId, UserId,
};
use payplan_core::shared::money::Money;
use payplan_infra::migrator;
use payplan_infra::postgres::{connect, PgConfig};
use payplan_infra::purchase_writer::PgPurchaseWriter;
use rust_decimal_macros::dec;
use serde_json::json;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let cfg = PgConfig::from_env().expect("DATABASE_URL");
    let pool = connect(&cfg).await.expect("connect");
    migrator::run(&pool).await.expect("migrations");
    pool
}

fn fresh_id<T: From<uuid::Uuid>>() -> T {
    T::from(uuid::Uuid::now_v7())
}

fn sample_purchase(user_id: UserId, package_id: PackageId) -> Purchase {
    Purchase {
        id: fresh_id::<PurchaseId>(),
        user_id,
        package_id,
        sponsor_user_id: None,
        gross: Money::new(dec!(99), "USD"),
        net: Money::new(dec!(99), "USD"),
        status: PurchaseStatus::Paid,
        purchased_at: Utc::now(),
    }
}

fn sample_subscription(
    user_id: UserId,
    package_id: PackageId,
    billing_plan_id: payplan_core::shared::ids::BillingPlanId,
) -> Subscription {
    Subscription {
        id: fresh_id::<payplan_core::shared::ids::SubscriptionId>(),
        user_id,
        package_id,
        billing_plan_id,
        status: SubscriptionStatus::Active,
        current_period: Some(payplan_core::shared::period::Period {
            starts_at: Utc::now(),
            ends_at: None,
        }),
        cancelled_at: None,
        created_at: Utc::now(),
    }
}

fn sample_entitlement(
    user_id: UserId,
    package_id: PackageId,
    catalog_item_id: payplan_core::shared::ids::CatalogItemId,
) -> Entitlement {
    Entitlement {
        id: fresh_id::<payplan_core::shared::ids::EntitlementId>(),
        user_id,
        package_id,
        catalog_item_id,
        source_purchase_id: None,
        source_subscription_id: None,
        status: EntitlementStatus::Active,
        starts_at: Utc::now(),
        ends_at: None,
        revoked_at: None,
    }
}

fn sample_enrollment(
    user_id: UserId,
    package_id: PackageId,
    purchase_id: PurchaseId,
) -> Enrollment {
    Enrollment {
        id: EnrollmentId::new(),
        user_id,
        package_id,
        purchase_id,
        sponsor_user_id: None,
        status: EnrollmentStatus::Active,
        joined_at: Utc::now(),
    }
}

fn sample_event() -> DomainEvent {
    DomainEvent {
        id: fresh_id::<payplan_core::shared::ids::EventId>(),
        event_type: EventType::PackagePurchased,
        payload: json!({"user_id": UserId::new(), "package_id": PackageId::new()}),
        created_at: Utc::now(),
    }
}

fn sample_ledger(
    user_id: UserId,
    event_id: payplan_core::shared::ids::EventId,
) -> RewardLedgerEntry {
    RewardLedgerEntry {
        id: LedgerEntryId::new(),
        user_id,
        enrollment_id: None,
        package_id: None,
        source_module: "test".into(),
        source_event_id: Some(event_id),
        amount: Money::new(dec!(1), "USD"),
        points: 0,
        status: LedgerStatus::Pending,
        reason: "test".into(),
        created_at: Utc::now(),
    }
}

async fn seed_user_pkg(
    pool: &PgPool,
) -> (
    UserId,
    PackageId,
    payplan_core::shared::ids::BillingPlanId,
    payplan_core::shared::ids::CatalogItemId,
) {
    let user_id = UserId::new();
    let item_id = payplan_core::shared::ids::CatalogItemId::new();
    let billing_plan_id = payplan_core::shared::ids::BillingPlanId::new();
    let package_id = PackageId::new();

    sqlx::query("INSERT INTO users (id, email, password_hash, role) VALUES ($1, $2, 'ph', 'user')")
        .bind(user_id)
        .bind(format!("user-{}@t.local", uuid::Uuid::now_v7().simple()))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO catalog_items (id, name, item_type, sku, status, metadata) VALUES ($1, 'I', 'service', 's', 'active', '{}')")
        .bind(item_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO billing_plans (id, catalog_item_id, billing_type, price_amount, currency, active) VALUES ($1, $2, 'one_time', 1, 'USD', true)")
        .bind(billing_plan_id)
        .bind(item_id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO packages (id, name, status, metadata) VALUES ($1, 'P', 'active', '{}')")
        .bind(package_id)
        .execute(pool)
        .await
        .unwrap();
    (user_id, package_id, billing_plan_id, item_id)
}

async fn truncate_all(pool: &PgPool) {
    sqlx::query("TRUNCATE TABLE reward_ledger, event_log, entitlements, enrollments, purchases, subscriptions, package_items, packages, pay_plan_stack_modules, pay_plan_stacks, billing_plans, catalog_items, users RESTART IDENTITY CASCADE")
        .execute(pool).await.unwrap();
}

#[tokio::test]
async fn atomic_write_persists_all_rows() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (user_id, package_id, billing_plan_id, item_id) =
        seed_user_pkg(&pool).await;

    let writer = PgPurchaseWriter::new(pool.clone());
    let purchase = sample_purchase(user_id, package_id);
    let sub = sample_subscription(user_id, package_id, billing_plan_id);
    let ent = sample_entitlement(user_id, package_id, item_id);
    let enrollment = sample_enrollment(user_id, package_id, purchase.id);
    let event = sample_event();
    let ledger = sample_ledger(user_id, event.id);

    let writes = PurchaseWrites {
        subscriptions: &[sub],
        entitlements: &[ent],
        purchase: &purchase,
        enrollment: &enrollment,
        events: &[event],
        ledger: &[ledger],
        module_state_changes: &[],
        projector: None,
        event_projector: None,
    };
    writer.write(writes).await.expect("write");

    assert_eq!(count(&pool, "purchases").await, 1);
    assert_eq!(count(&pool, "subscriptions").await, 1);
    assert_eq!(count(&pool, "entitlements").await, 1);
    assert_eq!(count(&pool, "enrollments").await, 1);
    assert_eq!(count(&pool, "event_log").await, 1);
    assert_eq!(count(&pool, "reward_ledger").await, 1);
}

#[tokio::test]
async fn atomic_write_rolls_back_on_failure() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (user_id, package_id, billing_plan_id, item_id) =
        seed_user_pkg(&pool).await;

    let dup_purchase_id = PurchaseId::new();
    sqlx::query("INSERT INTO purchases (id, user_id, package_id, gross_amount, net_amount, currency, status, purchased_at) VALUES ($1, $2, $3, 1, 1, 'USD', 'paid', NOW())")
        .bind(dup_purchase_id)
        .bind(user_id)
        .bind(package_id)
        .execute(&pool)
        .await
        .unwrap();

    let writer = PgPurchaseWriter::new(pool.clone());
    let mut purchase = sample_purchase(user_id, package_id);
    purchase.id = dup_purchase_id; // collide
    let sub = sample_subscription(user_id, package_id, billing_plan_id);
    let ent = sample_entitlement(user_id, package_id, item_id);
    let enrollment = sample_enrollment(user_id, package_id, purchase.id);
    let event = sample_event();
    let ledger = sample_ledger(user_id, event.id);

    let writes = PurchaseWrites {
        subscriptions: &[sub],
        entitlements: &[ent],
        purchase: &purchase,
        enrollment: &enrollment,
        events: &[event],
        ledger: &[ledger],
        module_state_changes: &[],
        projector: None,
        event_projector: None,
    };
    let result: AppResult<()> = writer.write(writes).await;
    assert!(result.is_err(), "expected PK collision to fail the write");

    assert_eq!(count(&pool, "purchases").await, 1);
    assert_eq!(
        count(&pool, "subscriptions").await,
        0,
        "rollback left no subscription"
    );
    assert_eq!(
        count(&pool, "entitlements").await,
        0,
        "rollback left no entitlement"
    );
    assert_eq!(
        count(&pool, "enrollments").await,
        0,
        "rollback left no enrollment"
    );
    assert_eq!(
        count(&pool, "event_log").await,
        0,
        "rollback left no event"
    );
    assert_eq!(
        count(&pool, "reward_ledger").await,
        0,
        "rollback left no ledger"
    );
}

async fn count(pool: &PgPool, table: &str) -> i64 {
    let q = format!("SELECT COUNT(*) FROM {table}");
    sqlx::query_scalar(&q)
        .fetch_one(pool)
        .await
        .unwrap_or(0)
}
