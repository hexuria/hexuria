//! Regression test for the transactional-integrity fix.
//!
//! Before the fix, the purchase handler inserted `purchases` and `enrollments`
//! before running the engine cascade. If the cascade errored (e.g. invalid
//! stack), orphan rows remained in the DB. The fix defers all inserts until
//! after the cascade succeeds.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use payplan_app::commands::{handle_purchase_package, PurchaseDeps, PurchasePackageCommand};
use payplan_app::ports::{
    CatalogRepo, CompanyRepo, EnrollmentRepo, EntitlementRepo, EventStore, PackageRepo,
    PayPlanStackRepo, PurchaseRepo, RewardLedgerStore, SubscriptionRepo, UserRepo,
};
use payplan_core::payplan::events::DomainEvent;
use payplan_core::payplan::ledger::RewardLedgerEntry;
use payplan_core::payplan::registry::ModuleRegistry;
use payplan_core::payplan::stack::{PayPlanStack, PayPlanStackStatus, StackModule};
use payplan_core::platform::catalog::{
    BillingPlan, BillingType, CatalogItem, CatalogItemStatus, CatalogItemType,
};
use payplan_core::platform::company::{Company, CompanyStatus};
use payplan_core::platform::package::{Package, PackageItem, PackageItemRole, PackageStatus};
use payplan_core::shared::ids::{
    BillingPlanId, CatalogItemId, CompanyId, EnrollmentId, PackageId, PayPlanStackId, PurchaseId,
    SubscriptionId, UserId,
};
use rust_decimal_macros::dec;
use serde_json::json;

#[derive(Default)]
struct InMemoryStores {
    companies: Mutex<HashMap<uuid::Uuid, Company>>,
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

#[async_trait]
impl CompanyRepo for InMemoryStores {
    async fn insert(&self, c: &Company, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<()> {
        self.companies.lock().unwrap().insert(c.id.0, c.clone());
        Ok(())
    }
    async fn get(&self, id: CompanyId, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<Option<Company>> {
        Ok(self.companies.lock().unwrap().get(&id.0).cloned())
    }
    async fn list(&self, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<Vec<Company>> {
        Ok(self.companies.lock().unwrap().values().cloned().collect())
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
        Ok(self.users.lock().unwrap().get(&id.0).cloned())
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
    async fn insert_item(&self, i: &CatalogItem, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<()> {
        self.catalog_items.lock().unwrap().insert(i.id.0, i.clone());
        Ok(())
    }
    async fn get_item(
        &self,
        id: CatalogItemId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<CatalogItem>> {
        Ok(self.catalog_items.lock().unwrap().get(&id.0).cloned())
    }
    async fn list_items(&self, c: CompanyId, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<Vec<CatalogItem>> {
        Ok(self
            .catalog_items
            .lock()
            .unwrap()
            .values()
            .filter(|i| i.company_id == c)
            .cloned()
            .collect())
    }
    async fn insert_billing_plan(&self, p: &BillingPlan, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<()> {
        self.billing_plans.lock().unwrap().insert(p.id.0, p.clone());
        Ok(())
    }
    async fn get_billing_plan(
        &self,
        id: BillingPlanId,
        _conn: &mut sqlx::PgConnection,
    ) -> payplan_app::error::AppResult<Option<BillingPlan>> {
        Ok(self.billing_plans.lock().unwrap().get(&id.0).cloned())
    }
}

#[async_trait]
impl PackageRepo for InMemoryStores {
    async fn insert(&self, p: &Package, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<()> {
        self.packages.lock().unwrap().insert(p.id.0, p.clone());
        Ok(())
    }
    async fn get(&self, id: PackageId, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<Option<Package>> {
        Ok(self.packages.lock().unwrap().get(&id.0).cloned())
    }
    async fn list(&self, _c: CompanyId, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<Vec<Package>> {
        Ok(self.packages.lock().unwrap().values().cloned().collect())
    }
}

#[async_trait]
impl PayPlanStackRepo for InMemoryStores {
    async fn insert(&self, s: &PayPlanStack, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<()> {
        self.stacks.lock().unwrap().insert(s.id.0, s.clone());
        Ok(())
    }
    async fn get(&self, id: PayPlanStackId, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<Option<PayPlanStack>> {
        Ok(self.stacks.lock().unwrap().get(&id.0).cloned())
    }
    async fn next_version(&self, _c: CompanyId, _name: &str, _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<u32> {
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
        Ok(self.purchases.lock().unwrap().get(&id.0).cloned())
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
        Ok(self.subscriptions.lock().unwrap().get(&id.0).cloned())
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
        Ok(self.enrollments.lock().unwrap().get(&id.0).cloned())
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
    async fn append(&self, events: &[DomainEvent], _conn: &mut sqlx::PgConnection) -> payplan_app::error::AppResult<()> {
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
    company_id: CompanyId,
    stack_modules: Vec<StackModule>,
) -> (PackageId, UserId) {
    let item = CatalogItem {
        id: CatalogItemId::new(),
        company_id,
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
        billing_type: BillingType::OneTime,
        price: payplan_core::shared::money::Money::new(dec!(99.00), "USD"),
        recurring: None,
        active: true,
        created_at: Utc::now(),
    };
    let stack = PayPlanStack {
        id: PayPlanStackId::new(),
        company_id,
        name: "Test Stack".into(),
        version: 1,
        status: PayPlanStackStatus::Active,
        modules: stack_modules,
        created_at: Utc::now(),
    };
    let package = Package {
        id: PackageId::new(),
        company_id,
        name: "Test Package".into(),
        description: None,
        status: PackageStatus::Active,
        pay_plan_stack_id: Some(stack.id),
        default_billing_plan_id: Some(billing.id),
        metadata: json!({}),
        created_at: Utc::now(),
        items: vec![PackageItem {
            catalog_item_id: item.id,
            billing_plan_id: billing.id,
            quantity: 1,
            role: PackageItemRole::Included,
            is_commissionable: true,
            commissionable_volume: 50,
            points_value: 5,
        }],
    };
    let user = payplan_core::platform::user::User {
        id: UserId::new(),
        email: "buyer@example.com".into(),
        password_hash: "$argon2id$placeholder".into(),
        email_verified: true,
        role: payplan_core::platform::user::UserRole::User,
        company_id: None,
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

    (package_id, user_id)
}

fn empty_registry() -> Arc<ModuleRegistry> {
    Arc::new(ModuleRegistry::new())
}

#[tokio::test]
async fn empty_stack_leaves_no_orphan_rows() {
    let stores = Arc::new(InMemoryStores::default());
    let company_id = CompanyId::new();
    stores.companies.lock().unwrap().insert(
        company_id.0,
        Company {
            id: company_id,
            name: "Acme".into(),
            slug: "acme".into(),
            status: CompanyStatus::Active,
            settings: json!({}),
            created_at: Utc::now(),
        },
    );
    // Stack with NO modules triggers the "stack has no modules" validation error.
    let (package_id, user_id) = seed(&stores, company_id, vec![]);

    let cmd = PurchasePackageCommand {
        company_id,
        user_id,
        package_id,
        sponsor_user_id: None,
        payment_currency: "USD".into(),
        gross_amount: dec!(99.00),
    };
    let deps = PurchaseDeps {
        pool: &sqlx::PgPool::connect_lazy("postgres://x@y/z").unwrap(),
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
        events: stores.as_ref(),
        ledger: stores.as_ref(),
        registry: empty_registry(),
    };
    let err = handle_purchase_package(cmd, &deps).await.unwrap_err();
    assert!(
        matches!(err, payplan_app::error::AppError::Core(_)),
        "got {err:?}"
    );

    // After the fix, NO rows should have been inserted when the engine fails.
    assert_eq!(
        stores.purchases.lock().unwrap().len(),
        0,
        "no orphan purchases"
    );
    assert_eq!(
        stores.enrollments.lock().unwrap().len(),
        0,
        "no orphan enrollments"
    );
    assert_eq!(
        stores.entitlements.lock().unwrap().len(),
        0,
        "no orphan entitlements"
    );
    assert_eq!(
        stores.subscriptions.lock().unwrap().len(),
        0,
        "no orphan subscriptions"
    );
    assert_eq!(stores.events.lock().unwrap().len(), 0, "no orphan events");
    assert_eq!(stores.ledger.lock().unwrap().len(), 0, "no orphan ledger");
}

#[tokio::test]
async fn unknown_billing_plan_returns_validation_before_insert() {
    let stores = Arc::new(InMemoryStores::default());
    let company_id = CompanyId::new();
    stores.companies.lock().unwrap().insert(
        company_id.0,
        Company {
            id: company_id,
            name: "Acme".into(),
            slug: "acme".into(),
            status: CompanyStatus::Active,
            settings: json!({}),
            created_at: Utc::now(),
        },
    );

    let item = CatalogItem {
        id: CatalogItemId::new(),
        company_id,
        name: "Item".into(),
        description: None,
        item_type: CatalogItemType::Service,
        sku: None,
        status: CatalogItemStatus::Active,
        metadata: json!({}),
        created_at: Utc::now(),
    };
    let package = Package {
        id: PackageId::new(),
        company_id,
        name: "Bad Package".into(),
        description: None,
        status: PackageStatus::Active,
        pay_plan_stack_id: None,
        default_billing_plan_id: None,
        metadata: json!({}),
        created_at: Utc::now(),
        items: vec![PackageItem {
            catalog_item_id: item.id,
            billing_plan_id: BillingPlanId::new(), // doesn't exist in billing_plans
            quantity: 1,
            role: PackageItemRole::Included,
            is_commissionable: true,
            commissionable_volume: 0,
            points_value: 0,
        }],
    };
    let user = payplan_core::platform::user::User {
        id: UserId::new(),
        email: "buyer@example.com".into(),
        password_hash: "ph".into(),
        email_verified: true,
        role: payplan_core::platform::user::UserRole::User,
        company_id: None,
        created_at: Utc::now(),
    };
    let cmd = PurchasePackageCommand {
        company_id,
        user_id: user.id,
        package_id: package.id,
        sponsor_user_id: None,
        payment_currency: "USD".into(),
        gross_amount: dec!(99.00),
    };

    stores.catalog_items.lock().unwrap().insert(item.id.0, item);
    stores
        .packages
        .lock()
        .unwrap()
        .insert(package.id.0, package);
    stores.users.lock().unwrap().insert(user.id.0, user);

    let deps = PurchaseDeps {
        pool: &sqlx::PgPool::connect_lazy("postgres://x@y/z").unwrap(),
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
        events: stores.as_ref(),
        ledger: stores.as_ref(),
        registry: empty_registry(),
    };
    let err = handle_purchase_package(cmd, &deps).await.unwrap_err();
    assert!(
        matches!(err, payplan_app::error::AppError::NotFound(_)),
        "got {err:?}"
    );

    assert_eq!(stores.purchases.lock().unwrap().len(), 0);
    assert_eq!(stores.enrollments.lock().unwrap().len(), 0);
    assert_eq!(stores.entitlements.lock().unwrap().len(), 0);
    assert_eq!(stores.events.lock().unwrap().len(), 0);
}
