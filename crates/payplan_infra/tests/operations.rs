//! Integration tests for the operations jobs.
//!
//! Requires a real Postgres at `DATABASE_URL`. Gated behind the `integration`
//! feature so the default `cargo test` run doesn't need a DB.
//!
//! Run with:
//!   `DATABASE_URL=postgres://... cargo test -p payplan_infra --features integration -- --include-ignored`

#![cfg(feature = "integration")]

use chrono::Utc;
use payplan_core::payplan::events::EventType;
use payplan_core::payplan::registry::ModuleRegistry;
use payplan_core::payplan::runner::StackRunner;
use payplan_core::shared::ids::CompanyId;
use payplan_infra::aggregate_repos::{
    PgCatalogRepo, PgEnrollmentRepo, PgPackageRepo, PgPayPlanStackRepo,
};
use payplan_infra::event_store::PgEventStore;
use payplan_infra::ledger_store::PgLedgerStore;
use payplan_infra::migrator;
use payplan_infra::operations::{run_renewals, run_royal_pot_distribution};
use payplan_infra::postgres::{connect, PgConfig};
use payplan_infra::repos::PgUserRepo;
use sqlx::PgPool;
use std::sync::Arc;

async fn pool() -> PgPool {
    let cfg = PgConfig::from_env().expect("DATABASE_URL");
    let pool = connect(&cfg).await.expect("connect");
    migrator::run(&pool).await.expect("migrations");
    pool
}

#[tokio::test]
async fn renewals_process_due_subscriptions() {
    let pool = pool().await;
    // Truncate to start clean.
    sqlx::query("TRUNCATE TABLE reward_ledger, event_log, entitlements, enrollments, purchases, subscriptions, package_items, packages, pay_plan_stack_modules, pay_plan_stacks, billing_plans, catalog_items, users, companies RESTART IDENTITY CASCADE")
        .execute(&pool).await.unwrap();

    let company_id = CompanyId::new();
    let user_id = payplan_core::shared::ids::UserId::new();
    let pkg = payplan_core::shared::ids::PackageId::new();

    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, 'T', 't')")
        .bind(company_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, email, password_hash, role, company_id) VALUES ($1, 'u@t', 'ph', 'user', $2)")
        .bind(user_id)
        .bind(company_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO packages (id, company_id, name, status, metadata) VALUES ($1, $2, 'P', 'active', '{}')")
        .bind(pkg)
        .bind(company_id)
        .execute(&pool)
        .await
        .unwrap();
    // Insert a subscription whose current_period_end is in the past.
    // Pre-create a billing_plan + catalog_item (FK targets) so the
    // subscription insert succeeds regardless of test ordering.
    let item_id = payplan_core::shared::ids::CatalogItemId::new();
    let billing_plan_id = payplan_core::shared::ids::BillingPlanId::new();
    sqlx::query(
        "INSERT INTO catalog_items (id, company_id, name, item_type, sku, status, metadata) VALUES ($1, $2, 'I', 'service', 's', 'active', '{}')",
    )
    .bind(item_id)
    .bind(company_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO billing_plans (id, catalog_item_id, billing_type, price_amount, currency, recurrence_interval, active) VALUES ($1, $2, 'recurring', 1, 'USD', 'monthly', true)",
    )
    .bind(billing_plan_id)
    .bind(item_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO subscriptions (id, company_id, user_id, package_id, billing_plan_id, status, current_period_start, current_period_end, created_at) VALUES ($1, $2, $3, $4, $5, 'active', NOW() - INTERVAL '2 days', NOW() - INTERVAL '1 day', NOW())",
    )
    .bind(payplan_core::shared::ids::SubscriptionId::new())
    .bind(company_id)
    .bind(user_id)
    .bind(pkg)
    .bind(billing_plan_id)
    .execute(&pool)
    .await
    .unwrap();

    // Build a minimal PurchaseDeps pointing at this DB.
    let registry = Arc::new(ModuleRegistry::new());
    let events: Arc<dyn payplan_app::ports::EventStore> = Arc::new(PgEventStore::new(pool.clone()));
    let ledger: Arc<dyn payplan_app::ports::RewardLedgerStore> =
        Arc::new(PgLedgerStore::new(pool.clone()));
    let catalog_repo = PgCatalogRepo::new(pool.clone());
    let enrollments = PgEnrollmentRepo::new(pool.clone());
    let packages = PgPackageRepo::new(pool.clone());
    let pay_plan_stacks = PgPayPlanStackRepo::new(pool.clone());
    let purchases: Arc<dyn payplan_app::ports::PurchaseRepo> = Arc::new(
        payplan_infra::aggregate_repos::PgPurchaseRepo::new(pool.clone()),
    );
    let subs: Arc<dyn payplan_app::ports::SubscriptionRepo> = Arc::new(
        payplan_infra::aggregate_repos::PgSubscriptionRepo::new(pool.clone()),
    );
    let ents: Arc<dyn payplan_app::ports::EntitlementRepo> = Arc::new(
        payplan_infra::aggregate_repos::PgEntitlementRepo::new(pool.clone()),
    );
    let companies: Arc<dyn payplan_app::ports::CompanyRepo> =
        Arc::new(payplan_infra::repos::PgCompanyRepo::new(pool.clone()));
    let users_repo = PgUserRepo::new(pool.clone());

    let deps = payplan_app::commands::PurchaseDeps {
        pool: &pool,
        packages: &packages,
        catalog: &catalog_repo,
        purchases: purchases.as_ref(),
        subscriptions: subs.as_ref(),
        entitlements: ents.as_ref(),
        enrollments: &enrollments,
        pay_plan_stacks: &pay_plan_stacks,
        events: events.as_ref(),
        ledger: ledger.as_ref(),
        registry,
        purchase_writer: None,
        module_state_store: None,
        projector: None,
        event_projector: None,
    };

    let processed = run_renewals(&pool, &deps).await.expect("renewals");
    assert_eq!(processed, 1);

    // Renewal should have emitted SubscriptionRenewed.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_log WHERE event_type = $1 AND company_id = $2",
    )
    .bind(EventType::SubscriptionRenewed.as_str())
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);

    let _ = (
        companies,
        users_repo,
        StackRunner::new(ModuleRegistry::new()),
    );
}
