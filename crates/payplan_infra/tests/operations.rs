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
    let ledger: Arc<dyn payplan_app::ports::RewardLedgerStore> = Arc::new(PgLedgerStore::new());
    let catalog_repo = PgCatalogRepo::new();
    let enrollments = PgEnrollmentRepo::new();
    let packages = PgPackageRepo::new();
    let pay_plan_stacks = PgPayPlanStackRepo::new();
    let purchases: Arc<dyn payplan_app::ports::PurchaseRepo> =
        Arc::new(payplan_infra::aggregate_repos::PgPurchaseRepo::new());
    let subs: Arc<dyn payplan_app::ports::SubscriptionRepo> =
        Arc::new(payplan_infra::aggregate_repos::PgSubscriptionRepo::new());
    let ents: Arc<dyn payplan_app::ports::EntitlementRepo> =
        Arc::new(payplan_infra::aggregate_repos::PgEntitlementRepo::new());
    let companies: Arc<dyn payplan_app::ports::CompanyRepo> =
        Arc::new(payplan_infra::repos::PgCompanyRepo::new());
    let users_repo = PgUserRepo::new();

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

/// Task 1 regression: a package that HAS a pay plan stack must not crash the
/// renewal job by double-appending the trigger event. Before the fix,
/// `run_stack_against_event` appended the trigger once and then re-appended the
/// whole `emitted` vec (still containing the trigger with the same PRIMARY KEY
/// id) → duplicate-key violation. The stack-less test above dodged this by
/// early-returning. Here the package carries a real stack, so the with-stack
/// append path runs: the job must succeed and leave exactly ONE
/// `SubscriptionRenewed` row.
#[tokio::test]
async fn renewal_with_stack_does_not_double_append_trigger() {
    let pool = pool().await;
    sqlx::query("TRUNCATE TABLE reward_ledger, event_log, entitlements, enrollments, purchases, subscriptions, package_items, packages, pay_plan_stack_modules, pay_plan_stacks, billing_plans, catalog_items, users, companies RESTART IDENTITY CASCADE")
        .execute(&pool).await.unwrap();

    let company_id = CompanyId::new();
    let user_id = payplan_core::shared::ids::UserId::new();
    let pkg = payplan_core::shared::ids::PackageId::new();
    let stack_id = payplan_core::shared::ids::PayPlanStackId::new();

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

    // A pay plan stack with one registered module. flushline does NOT handle
    // SubscriptionRenewed, so it is skipped — but the stack being present means
    // run_stack_against_event runs its full append path instead of early
    // returning. That is exactly the path the dup-append bug lived on.
    sqlx::query("INSERT INTO pay_plan_stacks (id, company_id, name, version, status) VALUES ($1, $2, 'S', 1, 'active')")
        .bind(stack_id)
        .bind(company_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO pay_plan_stack_modules (id, stack_id, module_key, module_version, sort_order, config, active) VALUES ($1, $2, 'royal.flushline', '1.0.0', 10, '{}', true)")
        .bind(payplan_core::shared::ids::EventId::new().0)
        .bind(stack_id)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO packages (id, company_id, name, status, pay_plan_stack_id, metadata) VALUES ($1, $2, 'P', 'active', $3, '{}')")
        .bind(pkg)
        .bind(company_id)
        .bind(stack_id)
        .execute(&pool)
        .await
        .unwrap();

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

    // Registry MUST contain the module referenced by the stack, else the runner
    // errors with "module not registered" before reaching the append path.
    let mut reg = ModuleRegistry::new();
    reg.register(
        payplan_core::modules::royal::flushline_module::RoyalFlushlineModule::new(
            Default::default(),
        ),
    );
    let registry = Arc::new(reg);
    let events: Arc<dyn payplan_app::ports::EventStore> = Arc::new(PgEventStore::new(pool.clone()));
    let ledger: Arc<dyn payplan_app::ports::RewardLedgerStore> = Arc::new(PgLedgerStore::new());
    let catalog_repo = PgCatalogRepo::new();
    let enrollments = PgEnrollmentRepo::new();
    let packages = PgPackageRepo::new();
    let pay_plan_stacks = PgPayPlanStackRepo::new();
    let purchases: Arc<dyn payplan_app::ports::PurchaseRepo> =
        Arc::new(payplan_infra::aggregate_repos::PgPurchaseRepo::new());
    let subs: Arc<dyn payplan_app::ports::SubscriptionRepo> =
        Arc::new(payplan_infra::aggregate_repos::PgSubscriptionRepo::new());
    let ents: Arc<dyn payplan_app::ports::EntitlementRepo> =
        Arc::new(payplan_infra::aggregate_repos::PgEntitlementRepo::new());

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

    // Must NOT error (the dup-key crash would surface here).
    let processed = run_renewals(&pool, &deps)
        .await
        .expect("renewal with a stack must not crash on duplicate append");
    assert_eq!(processed, 1);

    // Exactly one trigger row — the trigger is persisted once, not twice/thrice.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_log WHERE event_type = $1 AND company_id = $2",
    )
    .bind(EventType::SubscriptionRenewed.as_str())
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "trigger event appended exactly once");
}

/// An event projector that always fails — used to prove the Path B write block
/// rolls back as a unit (Task 10).
struct FailingEventProjector;

#[async_trait::async_trait]
impl payplan_app::ports::EventProjector for FailingEventProjector {
    async fn project(
        &self,
        _events: &[payplan_core::payplan::events::DomainEvent],
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        Err(payplan_app::error::AppError::Infra(
            "injected projector failure".into(),
        ))
    }
}

/// Task 10: the Path B write+project block runs in one transaction. If a
/// projector fails mid-sequence, the event append that preceded it in the same
/// transaction must roll back — no `event_log` row is left behind.
#[tokio::test]
async fn failing_projector_rolls_back_event_append() {
    let pool = pool().await;
    sqlx::query("TRUNCATE TABLE reward_ledger, event_log, entitlements, enrollments, purchases, subscriptions, package_items, packages, pay_plan_stack_modules, pay_plan_stacks, billing_plans, catalog_items, users, companies RESTART IDENTITY CASCADE")
        .execute(&pool).await.unwrap();

    let company_id = CompanyId::new();
    let user_id = payplan_core::shared::ids::UserId::new();
    let pkg = payplan_core::shared::ids::PackageId::new();
    let stack_id = payplan_core::shared::ids::PayPlanStackId::new();

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
    sqlx::query("INSERT INTO pay_plan_stacks (id, company_id, name, version, status) VALUES ($1, $2, 'S', 1, 'active')")
        .bind(stack_id)
        .bind(company_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO pay_plan_stack_modules (id, stack_id, module_key, module_version, sort_order, config, active) VALUES ($1, $2, 'royal.flushline', '1.0.0', 10, '{}', true)")
        .bind(payplan_core::shared::ids::EventId::new().0)
        .bind(stack_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO packages (id, company_id, name, status, pay_plan_stack_id, metadata) VALUES ($1, $2, 'P', 'active', $3, '{}')")
        .bind(pkg)
        .bind(company_id)
        .bind(stack_id)
        .execute(&pool)
        .await
        .unwrap();

    let item_id = payplan_core::shared::ids::CatalogItemId::new();
    let billing_plan_id = payplan_core::shared::ids::BillingPlanId::new();
    sqlx::query("INSERT INTO catalog_items (id, company_id, name, item_type, sku, status, metadata) VALUES ($1, $2, 'I', 'service', 's', 'active', '{}')")
        .bind(item_id)
        .bind(company_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO billing_plans (id, catalog_item_id, billing_type, price_amount, currency, recurrence_interval, active) VALUES ($1, $2, 'recurring', 1, 'USD', 'monthly', true)")
        .bind(billing_plan_id)
        .bind(item_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO subscriptions (id, company_id, user_id, package_id, billing_plan_id, status, current_period_start, current_period_end, created_at) VALUES ($1, $2, $3, $4, $5, 'active', NOW() - INTERVAL '2 days', NOW() - INTERVAL '1 day', NOW())")
        .bind(payplan_core::shared::ids::SubscriptionId::new())
        .bind(company_id)
        .bind(user_id)
        .bind(pkg)
        .bind(billing_plan_id)
        .execute(&pool)
        .await
        .unwrap();

    let mut reg = ModuleRegistry::new();
    reg.register(
        payplan_core::modules::royal::flushline_module::RoyalFlushlineModule::new(
            Default::default(),
        ),
    );
    let registry = Arc::new(reg);
    let events: Arc<dyn payplan_app::ports::EventStore> = Arc::new(PgEventStore::new(pool.clone()));
    let ledger: Arc<dyn payplan_app::ports::RewardLedgerStore> = Arc::new(PgLedgerStore::new());
    let catalog_repo = PgCatalogRepo::new();
    let enrollments = PgEnrollmentRepo::new();
    let packages = PgPackageRepo::new();
    let pay_plan_stacks = PgPayPlanStackRepo::new();
    let purchases: Arc<dyn payplan_app::ports::PurchaseRepo> =
        Arc::new(payplan_infra::aggregate_repos::PgPurchaseRepo::new());
    let subs: Arc<dyn payplan_app::ports::SubscriptionRepo> =
        Arc::new(payplan_infra::aggregate_repos::PgSubscriptionRepo::new());
    let ents: Arc<dyn payplan_app::ports::EntitlementRepo> =
        Arc::new(payplan_infra::aggregate_repos::PgEntitlementRepo::new());
    let failing: Arc<dyn payplan_app::ports::EventProjector> = Arc::new(FailingEventProjector);

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
        event_projector: Some(failing.as_ref()),
    };

    // The job must surface the projector error.
    let result = run_renewals(&pool, &deps).await;
    assert!(result.is_err(), "failing projector must propagate an error");

    // And the transaction must have rolled back: NO SubscriptionRenewed row.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_log WHERE event_type = $1 AND company_id = $2",
    )
    .bind(EventType::SubscriptionRenewed.as_str())
    .bind(company_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0, "event append rolled back with the failed projector");
}
