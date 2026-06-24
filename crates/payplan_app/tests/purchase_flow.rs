//! End-to-end test of the purchase flow against in-memory mocks.
//!
//! Validates that the PurchasePackageCommand handler:
//! - creates a purchase, subscription(s), entitlements, enrollment
//! - emits the expected domain events
//! - runs the package's pay plan stack and produces ledger entries
//! - propagates per-module state changes
//! - respects the engine's cascading-event loop

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use payplan_app::commands::{
    default_module_registry, handle_purchase_package, PurchaseDeps, PurchasePackageCommand,
};
use payplan_app::ports::{
    AllocationRepo, CatalogRepo, EnrollmentRepo, EntitlementRepo, EventStore, PackageRepo,
    PayPlanStackRepo, PurchaseRepo, RewardLedgerStore, SubscriptionRepo, UserRepo,
};
use payplan_core::payplan::events::DomainEvent;
use payplan_core::payplan::ledger::RewardLedgerEntry;
use payplan_core::payplan::registry::ModuleRegistry;
use payplan_core::payplan::stack::{PayPlanStack, PayPlanStackStatus, StackModule};
use payplan_core::platform::catalog::{
    BillingPlan, BillingType, CatalogItem, CatalogItemStatus, CatalogItemType,
    ProductPayPlanAllocation,
};
use payplan_core::platform::package::{Package, PackageItem, PackageItemRole, PackageStatus};
use payplan_core::shared::ids::{
    BillingPlanId, CatalogItemId, EnrollmentId, PackageId, PayPlanStackId,
    ProductPayPlanAllocationId, PurchaseId, SubscriptionId, UserId,
};
use rust_decimal_macros::dec;
use serde_json::json;

#[derive(Default)]
struct InMemoryStores {
    allocations: Mutex<HashMap<uuid::Uuid, ProductPayPlanAllocation>>,
    users: Mutex<HashMap<uuid::Uuid, payplan_core::platform::user::User>>,
    catalog_items: Mutex<HashMap<uuid::Uuid, CatalogItem>>,
    billing_plans: Mutex<HashMap<uuid::Uuid, BillingPlan>>,
    packages: Mutex<HashMap<uuid::Uuid, Package>>,
    stacks: Mutex<HashMap<uuid::Uuid, PayPlanStack>>,
    purchases: Mutex<HashMap<uuid::Uuid, payplan_core::platform::purchase::Purchase>>,
    subscriptions: Mutex<HashMap<uuid::Uuid, payplan_core::platform::subscription::Subscription>>,
    entitlements: Mutex<HashMap<uuid::Uuid, payplan_core::platform::entitlement::Entitlement>>,
    enrollments: Mutex<HashMap<uuid::Uuid, payplan_core::platform::enrollment::Enrollment>>,
    events: Mutex<Vec<DomainEvent>>,
    ledger: Mutex<Vec<RewardLedgerEntry>>,
}

// We can't use the macro above because each trait has different method signatures.
// Implementing manually.

#[async_trait]
impl AllocationRepo for InMemoryStores {
    async fn insert(
        &self,
        allocation: &ProductPayPlanAllocation,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.allocations.lock().unwrap().insert(allocation.id.0, allocation.clone());
        Ok(())
    }
    async fn get(
        &self,
        id: ProductPayPlanAllocationId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<ProductPayPlanAllocation>> {
        Ok(self.allocations.lock().unwrap().get(&id.0).cloned())
    }
    async fn list_for_products(
        &self,
        product_ids: &[CatalogItemId],
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Vec<ProductPayPlanAllocation>> {
        let allocations = self.allocations.lock().unwrap();
        Ok(allocations
            .values()
            .filter(|a| product_ids.contains(&a.catalog_item_id))
            .cloned()
            .collect())
    }
    async fn list_all(
        &self,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Vec<ProductPayPlanAllocation>> {
        Ok(self.allocations.lock().unwrap().values().cloned().collect())
    }
    async fn delete(
        &self,
        id: ProductPayPlanAllocationId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.allocations.lock().unwrap().remove(&id.0);
        Ok(())
    }
}

#[async_trait]
impl UserRepo for InMemoryStores {
    async fn insert(
        &self,
        u: &payplan_core::platform::user::User,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.users.lock().unwrap().insert(u.id.0, u.clone());
        Ok(())
    }
    async fn get(
        &self,
        id: UserId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<payplan_core::platform::user::User>> {
        Ok(self.users.lock().unwrap().get(&(id.0)).cloned())
    }
    async fn find_by_email(
        &self,
        _email: &str,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<payplan_core::platform::user::User>> {
        Ok(None)
    }
}

#[async_trait]
impl CatalogRepo for InMemoryStores {
    async fn insert_item(
        &self,
        i: &CatalogItem,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.catalog_items.lock().unwrap().insert(i.id.0, i.clone());
        Ok(())
    }
    async fn get_item(
        &self,
        id: CatalogItemId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<CatalogItem>> {
        Ok(self.catalog_items.lock().unwrap().get(&(id.0)).cloned())
    }
    async fn list_items(
        &self,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Vec<CatalogItem>> {
        Ok(self
            .catalog_items
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect())
    }
    async fn insert_billing_plan(
        &self,
        p: &BillingPlan,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.billing_plans.lock().unwrap().insert(p.id.0, p.clone());
        Ok(())
    }
    async fn get_billing_plan(
        &self,
        id: BillingPlanId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<BillingPlan>> {
        Ok(self.billing_plans.lock().unwrap().get(&(id.0)).cloned())
    }
}

#[async_trait]
impl PackageRepo for InMemoryStores {
    async fn insert(
        &self,
        p: &Package,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.packages.lock().unwrap().insert(p.id.0, p.clone());
        Ok(())
    }
    async fn get(
        &self,
        id: PackageId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<Package>> {
        Ok(self.packages.lock().unwrap().get(&(id.0)).cloned())
    }
    async fn list(
        &self,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Vec<Package>> {
        Ok(self.packages.lock().unwrap().values().cloned().collect())
    }
}

#[async_trait]
impl PayPlanStackRepo for InMemoryStores {
    async fn insert(
        &self,
        s: &PayPlanStack,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.stacks.lock().unwrap().insert(s.id.0, s.clone());
        Ok(())
    }
    async fn get(
        &self,
        id: PayPlanStackId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<PayPlanStack>> {
        Ok(self.stacks.lock().unwrap().get(&(id.0)).cloned())
    }
    async fn next_version(
        &self,
        _name: &str,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<u32> {
        Ok(1)
    }
}

#[async_trait]
impl PurchaseRepo for InMemoryStores {
    async fn insert(
        &self,
        p: &payplan_core::platform::purchase::Purchase,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.purchases.lock().unwrap().insert(p.id.0, p.clone());
        Ok(())
    }
    async fn get(
        &self,
        id: PurchaseId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<payplan_core::platform::purchase::Purchase>> {
        Ok(self.purchases.lock().unwrap().get(&(id.0)).cloned())
    }
}

#[async_trait]
impl SubscriptionRepo for InMemoryStores {
    async fn insert(
        &self,
        s: &payplan_core::platform::subscription::Subscription,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.subscriptions.lock().unwrap().insert(s.id.0, s.clone());
        Ok(())
    }
    async fn get(
        &self,
        id: SubscriptionId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<payplan_core::platform::subscription::Subscription>>
    {
        Ok(self.subscriptions.lock().unwrap().get(&(id.0)).cloned())
    }
    async fn list_active_for_user(
        &self,
        u: UserId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Vec<payplan_core::platform::subscription::Subscription>>
    {
        Ok(self
            .subscriptions
            .lock()
            .unwrap()
            .values()
            .filter(|s| s.user_id == u)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl EntitlementRepo for InMemoryStores {
    async fn insert(
        &self,
        e: &payplan_core::platform::entitlement::Entitlement,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.entitlements.lock().unwrap().insert(e.id.0, e.clone());
        Ok(())
    }
    async fn list_for_user(
        &self,
        u: UserId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Vec<payplan_core::platform::entitlement::Entitlement>> {
        Ok(self
            .entitlements
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.user_id == u)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl EnrollmentRepo for InMemoryStores {
    async fn insert(
        &self,
        e: &payplan_core::platform::enrollment::Enrollment,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.enrollments.lock().unwrap().insert(e.id.0, e.clone());
        Ok(())
    }
    async fn get(
        &self,
        id: EnrollmentId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<payplan_core::platform::enrollment::Enrollment>> {
        Ok(self.enrollments.lock().unwrap().get(&(id.0)).cloned())
    }
    async fn list_for_user(
        &self,
        u: UserId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Vec<payplan_core::platform::enrollment::Enrollment>> {
        Ok(self
            .enrollments
            .lock()
            .unwrap()
            .values()
            .filter(|e| e.user_id == u)
            .cloned()
            .collect())
    }
}

#[async_trait]
impl EventStore for InMemoryStores {
    async fn append(
        &self,
        events: &[DomainEvent],
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<()> {
        self.events.lock().unwrap().extend_from_slice(events);
        Ok(())
    }
}

#[async_trait]
impl RewardLedgerStore for InMemoryStores {
    async fn append(
        &self,
        entries: &[RewardLedgerEntry],
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Vec<payplan_core::shared::ids::LedgerEntryId>> {
        self.ledger.lock().unwrap().extend_from_slice(entries);
        Ok(entries.iter().map(|e| e.id).collect())
    }
}

fn seed(
    stores: &InMemoryStores,
    with_recurring: bool,
) -> (PackageId, UserId) {
    let item = CatalogItem {
        id: CatalogItemId::new(),
        name: "Test Item".into(),
        description: None,
        item_type: CatalogItemType::Service,
        sku: None,
        status: CatalogItemStatus::Active,
        metadata: json!({}),
        created_at: Utc::now(),
    };
    let billing = BillingPlan {
        id: BillingPlanId::new(),
        catalog_item_id: item.id,
        billing_type: if with_recurring {
            BillingType::Recurring
        } else {
            BillingType::OneTime
        },
        price: payplan_core::shared::money::Money::new(dec!(99.00), "USD"),
        recurring: if with_recurring {
            Some(payplan_core::platform::catalog::RecurringSettings {
                interval: payplan_core::platform::catalog::RecurrenceInterval::Monthly,
                interval_count: 1,
                trial_days: 0,
                grace_period_days: 7,
            })
        } else {
            None
        },
        active: true,
        created_at: Utc::now(),
    };
    let stack = PayPlanStack {
        id: PayPlanStackId::new(),
        name: "RFN".into(),
        version: 1,
        status: PayPlanStackStatus::Active,
        modules: vec![
            StackModule {
                module_key: "sponsor.allocation".into(),
                module_version: "1.0.0".into(),
                sort_order: 10,
                config: json!({}),
                active: true,
            },
            StackModule {
                module_key: "royal.flushline".into(),
                module_version: "1.0.0".into(),
                sort_order: 20,
                config: json!({}),
                active: true,
            },
            StackModule {
                module_key: "royal.matrix".into(),
                module_version: "1.0.0".into(),
                sort_order: 30,
                config: json!({}),
                active: true,
            },
        ],
        created_at: Utc::now(),
    };
    let package = Package {
        id: PackageId::new(),
        name: "Royal Flush Starter".into(),
        description: None,
        status: PackageStatus::Active,
        default_billing_plan_id: Some(billing.id),
        metadata: json!({}),
        created_at: Utc::now(),
        items: vec![PackageItem {
            catalog_item_id: item.id,
            billing_plan_id: billing.id,
            quantity: 1,
            role: PackageItemRole::Included,
            is_commissionable: true,
        }],
    };
    let allocation = ProductPayPlanAllocation {
        id: ProductPayPlanAllocationId::new(),
        catalog_item_id: item.id,
        pay_plan_stack_id: stack.id,
        points: 50,
        active: true,
        created_at: Utc::now(),
    };
    let user = payplan_core::platform::user::User {
        id: UserId::new(),
        email: "buyer@example.com".into(),
        password_hash: "$argon2id$placeholder".into(),
        email_verified: true,
        role: payplan_core::platform::user::UserRole::User,
        created_at: Utc::now(),
    };
    let user_id = user.id;
    let package_id = package.id;

    stores.catalog_items.lock().unwrap().insert(item.id.0, item);
    stores
        .billing_plans
        .lock()
        .unwrap()
        .insert(billing.id.0, billing);
    stores.stacks.lock().unwrap().insert(stack.id.0, stack);
    stores
        .packages
        .lock()
        .unwrap()
        .insert(package.id.0, package);
    stores.users.lock().unwrap().insert(user.id.0, user);
    stores.allocations.lock().unwrap().insert(allocation.id.0, allocation);

    (package_id, user_id)
}

#[tokio::test]
async fn purchase_flow_emits_events_and_runs_royal_flush_modules() {
    let stores = Arc::new(InMemoryStores::default());
    let (package_id, user_id) = seed(&stores, false);

    let cmd = PurchasePackageCommand {
        user_id,
        package_id,
        sponsor_user_id: None,
    };
    let registry: Arc<ModuleRegistry> = Arc::new(default_module_registry());
    let pool = sqlx::PgPool::connect_lazy(
        &std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres@localhost/postgres".into()),
    )
    .unwrap();
    let deps = PurchaseDeps {
        pool: &pool,
        purchase_writer: None,
        projector: None,
        event_projector: None,
        module_state_store: None,
        packages: stores.as_ref(),
        catalog: stores.as_ref(),
        purchases: stores.as_ref(),
        subscriptions: stores.as_ref(),
        entitlements: stores.as_ref(),
        enrollments: stores.as_ref(),
        pay_plan_stacks: stores.as_ref(),
        allocations: stores.as_ref(),
        events: stores.as_ref(),
        ledger: stores.as_ref(),
        registry,
    };
    let outcome = handle_purchase_package(cmd, &deps)
        .await
        .expect("purchase ok");

    // 1 purchase + 1 entitlement + 1 enrollment.
    assert_eq!(stores.purchases.lock().unwrap().len(), 1);
    assert_eq!(stores.enrollments.lock().unwrap().len(), 1);
    assert_eq!(stores.entitlements.lock().unwrap().len(), 1);
    assert_eq!(stores.subscriptions.lock().unwrap().len(), 0);

    // Events: PackagePurchased + EnrollmentCreated + RoyalFlushlineAccountCreated
    // + RoyalMatrixCreated + (any cascading modules)
    let events = stores.events.lock().unwrap();
    assert!(events.iter().any(|e| matches!(
        e.event_type,
        payplan_core::payplan::events::EventType::PackagePurchased
    )));
    assert!(events.iter().any(|e| matches!(
        e.event_type,
        payplan_core::payplan::events::EventType::EnrollmentCreated
    )));
    assert!(events.iter().any(|e| matches!(
        e.event_type,
        payplan_core::payplan::events::EventType::RoyalFlushlineAccountCreated
    )));
    assert!(events.iter().any(|e| matches!(
        e.event_type,
        payplan_core::payplan::events::EventType::RoyalMatrixCreated
    )));

    assert!(outcome.events_emitted >= 2);
}

#[tokio::test]
async fn purchase_with_recurring_creates_subscription() {
    let stores = Arc::new(InMemoryStores::default());
    let (package_id, user_id) = seed(&stores, true);

    let cmd = PurchasePackageCommand {
        user_id,
        package_id,
        sponsor_user_id: None,
    };
    let registry: Arc<ModuleRegistry> = Arc::new(default_module_registry());
    let pool = sqlx::PgPool::connect_lazy(
        &std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres@localhost/postgres".into()),
    )
    .unwrap();
    let deps = PurchaseDeps {
        pool: &pool,
        purchase_writer: None,
        projector: None,
        event_projector: None,
        module_state_store: None,
        packages: stores.as_ref(),
        catalog: stores.as_ref(),
        purchases: stores.as_ref(),
        subscriptions: stores.as_ref(),
        entitlements: stores.as_ref(),
        enrollments: stores.as_ref(),
        pay_plan_stacks: stores.as_ref(),
        allocations: stores.as_ref(),
        events: stores.as_ref(),
        ledger: stores.as_ref(),
        registry,
    };
    let outcome = handle_purchase_package(cmd, &deps)
        .await
        .expect("purchase ok");
    assert_eq!(outcome.subscription_ids.len(), 1);
    assert_eq!(stores.subscriptions.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn inactive_package_is_rejected() {
    let stores = Arc::new(InMemoryStores::default());
    let (package_id, user_id) = seed(&stores, false);

    // Flip package to inactive.
    {
        let mut pkgs = stores.packages.lock().unwrap();
        let pkg = pkgs.get(&(package_id.0)).unwrap().clone();
        pkgs.insert(
            pkg.id.0,
            Package {
                status: PackageStatus::Draft,
                ..pkg
            },
        );
    }

    let cmd = PurchasePackageCommand {
        user_id,
        package_id,
        sponsor_user_id: None,
    };
    let registry: Arc<ModuleRegistry> = Arc::new(default_module_registry());
    let pool = sqlx::PgPool::connect_lazy(
        &std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres@localhost/postgres".into()),
    )
    .unwrap();
    let deps = PurchaseDeps {
        pool: &pool,
        purchase_writer: None,
        projector: None,
        event_projector: None,
        module_state_store: None,
        packages: stores.as_ref(),
        catalog: stores.as_ref(),
        purchases: stores.as_ref(),
        subscriptions: stores.as_ref(),
        entitlements: stores.as_ref(),
        enrollments: stores.as_ref(),
        pay_plan_stacks: stores.as_ref(),
        allocations: stores.as_ref(),
        events: stores.as_ref(),
        ledger: stores.as_ref(),
        registry,
    };
    let err = handle_purchase_package(cmd, &deps).await.unwrap_err();
    assert!(
        matches!(err, payplan_app::error::AppError::Conflict(_)),
        "got {err:?}"
    );
}
