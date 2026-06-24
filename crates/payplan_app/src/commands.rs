//! Application-level commands.
//!
//! Commands mutate platform state. They depend on the trait ports defined in
//! `ports.rs` and the pure domain types in `payplan_core`. Each command
//! resolves its inputs, writes through the ports, and returns the resulting
//! aggregate IDs to the caller.

use chrono::Utc;
use payplan_core::error::CoreResult;
use payplan_core::modules::binary::carryover_module::BinaryCarryoverModule;
use payplan_core::modules::binary::pairing_module::BinaryPairingModule;
use payplan_core::modules::binary::tree_module::BinaryTreeModule;
use payplan_core::modules::binary::volume_module::BinaryVolumeModule;
use payplan_core::modules::royal::duplication_module::RoyalAccountDuplicationModule;
use payplan_core::modules::royal::flushline_module::RoyalFlushlineModule;
use payplan_core::modules::royal::matrix_module::RoyalMatrixModule;
use payplan_core::modules::royal::pot_bonus_module::RoyalPotBonusModule;
use payplan_core::modules::sponsor::{SponsorAllocationConfig, SponsorAllocationModule};
use payplan_core::payplan::events::{DomainEvent, EventType};
use payplan_core::payplan::module::ModuleContext;
use payplan_core::payplan::registry::ModuleRegistry;
use payplan_core::payplan::runner::{StackRunResult, StackRunner, StateCache, StateChange};
use payplan_core::payplan::stack::PayPlanStack;
use payplan_core::platform::catalog::{BillingPlan, BillingType, ProductPayPlanAllocation};
use payplan_core::platform::enrollment::{Enrollment, EnrollmentStatus};
use payplan_core::platform::entitlement::{Entitlement, EntitlementStatus};
use payplan_core::platform::package::{Package, PackageStatus};
use payplan_core::platform::purchase::{Purchase, PurchaseStatus};
use payplan_core::platform::subscription::{Subscription, SubscriptionStatus};
use payplan_core::shared::ids::{
    BillingPlanId, CatalogItemId, EnrollmentId, PackageId, PayPlanStackId,
    ProductPayPlanAllocationId, PurchaseId, SubscriptionId, UserId,
};
use payplan_core::shared::money::Money;
use serde_json::json;
use tracing::info;

use crate::error::{AppError, AppResult};
use crate::ports::{
    AllocationRepo, CatalogRepo, EnrollmentRepo, EntitlementRepo, EventStore, PackageRepo,
    PayPlanStackRepo, PurchaseRepo, PurchaseWriter, RewardLedgerStore, SubscriptionRepo,
};

pub struct CreateCatalogItemCommand {
    pub name: String,
    pub description: Option<String>,
    pub sku: Option<String>,
    pub item_type: payplan_core::platform::catalog::CatalogItemType,
}

pub struct RegisterUserCommand {
    pub email: String,
    pub password: String,
    pub role: payplan_core::platform::user::UserRole,
}

pub struct CreateBillingPlanCommand {
    pub catalog_item_id: payplan_core::shared::ids::CatalogItemId,
    pub billing_type: payplan_core::platform::catalog::BillingType,
    pub price: payplan_core::shared::money::Money,
    pub recurring: Option<payplan_core::platform::catalog::RecurringSettings>,
}

pub struct CreatePackageCommand {
    pub name: String,
    pub description: Option<String>,
    pub items: Vec<payplan_core::platform::package::PackageItem>,
}

pub struct PurchasePackageCommand {
    pub user_id: UserId,
    pub package_id: PackageId,
    pub sponsor_user_id: Option<UserId>,
}

pub struct CreateProductPayPlanAllocationCommand {
    pub catalog_item_id: CatalogItemId,
    pub pay_plan_stack_id: PayPlanStackId,
    pub points: i64,
}

pub struct DeleteProductPayPlanAllocationCommand {
    pub id: ProductPayPlanAllocationId,
}

/// Default module registry with every built-in module registered.
#[must_use]
pub fn default_module_registry() -> ModuleRegistry {
    let mut r = ModuleRegistry::new();
    r.register(SponsorAllocationModule::new(
        SponsorAllocationConfig::default(),
    ));
    r.register(RoyalFlushlineModule::new(Default::default()));
    r.register(RoyalMatrixModule::new(Default::default()));
    r.register(RoyalPotBonusModule::new(Default::default()));
    r.register(RoyalAccountDuplicationModule::new(Default::default()));
    r.register(BinaryTreeModule::new(Default::default()));
    r.register(BinaryVolumeModule::new(Default::default()));
    r.register(BinaryPairingModule::new(Default::default()));
    r.register(BinaryCarryoverModule::new());
    r
}

// ----------------------------- Handlers -------------------------------------

pub async fn handle_create_catalog_item(
    cmd: CreateCatalogItemCommand,
    repo: &dyn CatalogRepo,
    pool: &sqlx::PgPool,
) -> AppResult<payplan_core::platform::catalog::CatalogItem> {
    let mut item = payplan_core::platform::catalog::CatalogItem::new(
        cmd.name.clone(),
        cmd.item_type,
    )
    .map_err(AppError::from)?;
    if let Some(sku) = &cmd.sku {
        if sku.trim().is_empty() {
            return Err(AppError::Validation("sku cannot be empty".into()));
        }
    }
    item.description = cmd.description;
    item.sku = cmd.sku;
    item.validate().map_err(AppError::from)?;
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
    repo.insert_item(&item, &mut conn).await?;
    Ok(item)
}

pub async fn handle_create_package(
    cmd: CreatePackageCommand,
    repo: &dyn PackageRepo,
    pool: &sqlx::PgPool,
) -> AppResult<Package> {
    let mut package = Package::new(cmd.name.clone(), cmd.items.clone())
        .map_err(AppError::from)?;
    package.description = cmd.description;
    package.status = PackageStatus::Active;
    package.validate().map_err(AppError::from)?;
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
    repo.insert(&package, &mut conn).await?;
    Ok(package)
}

pub async fn handle_register_user(
    cmd: RegisterUserCommand,
    users: &dyn crate::ports::UserRepo,
    passwords: &dyn crate::ports::PasswordPort,
    pool: &sqlx::PgPool,
) -> AppResult<payplan_core::platform::user::User> {
    let password_hash = passwords.hash(&cmd.password).await?;
    let mut user =
        payplan_core::platform::user::User::new(cmd.email, password_hash, cmd.role)
            .map_err(AppError::from)?;
    user.email_verified = false;
    user.validate().map_err(AppError::from)?;
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
    users.insert(&user, &mut conn).await?;
    Ok(user)
}

pub async fn handle_create_billing_plan(
    cmd: CreateBillingPlanCommand,
    repo: &dyn CatalogRepo,
    pool: &sqlx::PgPool,
) -> AppResult<BillingPlan> {
    let plan = BillingPlan::new(
        cmd.catalog_item_id,
        cmd.billing_type,
        cmd.price,
        cmd.recurring,
    )
    .map_err(AppError::from)?;
    plan.validate().map_err(AppError::from)?;
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
    repo.insert_billing_plan(&plan, &mut conn).await?;
    Ok(plan)
}

pub async fn handle_create_product_payplan_allocation(
    cmd: CreateProductPayPlanAllocationCommand,
    repo: &dyn AllocationRepo,
    pool: &sqlx::PgPool,
) -> AppResult<ProductPayPlanAllocation> {
    let allocation = ProductPayPlanAllocation {
        id: ProductPayPlanAllocationId::new(),
        catalog_item_id: cmd.catalog_item_id,
        pay_plan_stack_id: cmd.pay_plan_stack_id,
        points: cmd.points,
        active: true,
        created_at: Utc::now(),
    };
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
    repo.insert(&allocation, &mut conn).await?;
    Ok(allocation)
}

pub async fn handle_delete_product_payplan_allocation(
    cmd: DeleteProductPayPlanAllocationCommand,
    repo: &dyn AllocationRepo,
    pool: &sqlx::PgPool,
) -> AppResult<()> {
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
    repo.delete(cmd.id, &mut conn).await?;
    Ok(())
}

/// Run the full purchase flow.
///
/// Steps (per PRD §9):
/// 1. Validate package is active
/// 2. Load billing plans for each package item
/// 3. Create subscription if any item is recurring
/// 4. Grant entitlements for each package item
/// 5. Create purchase record
/// 6. Create enrollment into the package
/// 7. Emit `EnrollmentCreated`
/// 8. Load allocations for catalog items, group by stack
/// 9. For each stack with >0 points, emit `PackagePurchased` and run modules in cascade
/// 10. Persist emitted events + ledger entries
///
/// Returns the IDs of the created aggregates.
pub async fn handle_purchase_package(
    cmd: PurchasePackageCommand,
    deps: &PurchaseDeps<'_>,
) -> AppResult<PurchaseOutcome> {
    let mut conn = deps
        .pool
        .acquire()
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let package = deps
        .packages
        .get(cmd.package_id, &mut conn)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("package {} not found", cmd.package_id)))?;

    if !matches!(package.status, PackageStatus::Active) {
        return Err(AppError::Conflict(format!(
            "package {} is not active",
            cmd.package_id
        )));
    }

    let billing_plans = load_billing_plans(deps.catalog, &package, &mut conn).await?;
    validate_package_items(&package, &billing_plans)?;

    let now = Utc::now();

    // Build all aggregates in memory FIRST.
    let mut subscriptions: Vec<Subscription> = Vec::with_capacity(package.items.len());
    for (_item, plan) in package.items.iter().zip(billing_plans.iter()) {
        if matches!(plan.billing_type, BillingType::Recurring) {
            subscriptions.push(Subscription {
                id: SubscriptionId::new(),
                user_id: cmd.user_id,
                package_id: cmd.package_id,
                billing_plan_id: plan.id,
                status: SubscriptionStatus::Active,
                current_period: Some(payplan_core::shared::period::Period {
                    starts_at: now,
                    ends_at: None,
                }),
                cancelled_at: None,
                created_at: now,
            });
        }
    }

    let entitlements: Vec<Entitlement> = package
        .items
        .iter()
        .zip(billing_plans.iter())
        .map(|(item, _plan)| Entitlement {
            id: payplan_core::shared::ids::EntitlementId::new(),
            user_id: cmd.user_id,
            package_id: cmd.package_id,
            catalog_item_id: item.catalog_item_id,
            source_purchase_id: None,
            source_subscription_id: subscriptions.first().map(|s| s.id),
            status: EntitlementStatus::Active,
            starts_at: now,
            ends_at: None,
            revoked_at: None,
        })
        .collect();

    let gross = compute_package_price(&package, &billing_plans)?;
    let net = gross.clone();
    let purchase = Purchase {
        id: PurchaseId::new(),
        user_id: cmd.user_id,
        package_id: cmd.package_id,
        sponsor_user_id: cmd.sponsor_user_id,
        gross,
        net,
        status: PurchaseStatus::Paid,
        purchased_at: now,
    };
    purchase.validate().map_err(AppError::from)?;

    let enrollment = Enrollment {
        id: EnrollmentId::new(),
        user_id: cmd.user_id,
        package_id: cmd.package_id,
        purchase_id: purchase.id,
        sponsor_user_id: cmd.sponsor_user_id,
        status: EnrollmentStatus::Active,
        joined_at: now,
    };

    // Load active allocations for items in the package
    let product_ids: Vec<CatalogItemId> = package
        .items
        .iter()
        .map(|item| item.catalog_item_id)
        .collect();
    let allocations = deps
        .allocations
        .list_for_products(&product_ids, &mut conn)
        .await?;

    use std::collections::HashMap;
    let mut stack_allocations: HashMap<PayPlanStackId, Vec<ProductPayPlanAllocation>> = HashMap::new();
    for alloc in allocations {
        if alloc.active {
            stack_allocations
                .entry(alloc.pay_plan_stack_id)
                .or_default()
                .push(alloc);
        }
    }

    let mut emitted: Vec<DomainEvent> = vec![];
    let enrollment_created = domain_event(
        EventType::EnrollmentCreated,
        json!({
            "user_id": cmd.user_id,
            "package_id": cmd.package_id,
            "enrollment_id": enrollment.id,
            "sponsor_user_id": cmd.sponsor_user_id,
        }),
    );
    emitted.push(enrollment_created.clone());

    let mut ledger: Vec<payplan_core::payplan::ledger::RewardLedgerEntry> = vec![];
    let mut state_changes: Vec<StateChange> = vec![];

    let runner = StackRunner::new((*deps.registry).clone());

    // Pre-load existing module state. State is keyed per-aggregate:
    // enrollment-scoped modules (e.g. Flushline) under the enrollment id,
    // global-scoped modules under Uuid::nil().
    let mut state_cache = StateCache::new();
    if let Some(store) = deps.module_state_store {
        let by_enrollment = store.load_for_aggregate(enrollment.id.0, &mut conn).await?;
        for ((module_key, module_version), value) in &by_enrollment {
            state_cache.put(module_key, module_version, enrollment.id.0, value.clone());
        }
        let by_global = store.load_for_aggregate(uuid::Uuid::nil(), &mut conn).await?;
        for ((module_key, module_version), value) in &by_global {
            state_cache.put(
                module_key,
                module_version,
                uuid::Uuid::nil(),
                value.clone(),
            );
        }
    }

    for (stack_id, allocs) in stack_allocations {
        let mut total_points = 0i64;
        for alloc in allocs {
            let qty = package
                .items
                .iter()
                .find(|i| i.catalog_item_id == alloc.catalog_item_id)
                .map(|i| i.quantity)
                .unwrap_or(0);
            total_points += alloc.points * i64::from(qty);
        }

        if total_points > 0 {
            let package_purchased = domain_event(
                EventType::PackagePurchased,
                json!({
                    "user_id": cmd.user_id,
                    "package_id": cmd.package_id,
                    "purchase_id": purchase.id,
                    "enrollment_id": enrollment.id,
                    "sponsor_user_id": cmd.sponsor_user_id,
                    "points": total_points,
                    "volume": total_points,
                    "leg": "left",
                    "pot_contribution": total_points,
                }),
            );
            emitted.push(package_purchased.clone());

            let stack = deps
                .pay_plan_stacks
                .get(stack_id, &mut conn)
                .await?
                .ok_or_else(|| AppError::NotFound(format!("pay plan stack {stack_id} not found")))?;

            let mut stack_emitted = vec![
                enrollment_created.clone(),
                package_purchased,
            ];

            run_stack(
                &runner,
                &stack,
                &enrollment,
                &mut stack_emitted,
                &mut ledger,
                &mut state_changes,
                &mut state_cache,
            )
            .await?;

            // Append cascade-emitted events
            if stack_emitted.len() > 2 {
                emitted.extend(stack_emitted.into_iter().skip(2));
            }
        }
    }

    // Persist AFTER the engine has produced all events + ledger entries.
    if let Some(writer) = deps.purchase_writer {
        // Atomic path: all writes inside a single Postgres transaction.
        let writes = crate::ports::PurchaseWrites {
            subscriptions: &subscriptions,
            entitlements: &entitlements,
            purchase: &purchase,
            enrollment: &enrollment,
            events: &emitted,
            ledger: &ledger,
            module_state_changes: &state_changes,
            projector: deps.projector,
            event_projector: deps.event_projector,
        };
        writer.write(writes).await?;
    } else {
        // Non-atomic fallback (used by in-memory tests).
        for sub in &subscriptions {
            deps.subscriptions.insert(sub, &mut conn).await?;
        }
        for ent in &entitlements {
            deps.entitlements.insert(ent, &mut conn).await?;
        }
        deps.purchases.insert(&purchase, &mut conn).await?;
        deps.enrollments.insert(&enrollment, &mut conn).await?;
        deps.events.append(&emitted, &mut conn).await?;
        if !ledger.is_empty() {
            deps.ledger.append(&ledger, &mut conn).await?;
        }
    }

    info!(
        purchase_id = %purchase.id,
        enrollment_id = %enrollment.id,
        package_id = %cmd.package_id,
        events = emitted.len(),
        ledger = ledger.len(),
        state_changes = state_changes.len(),
        "purchase flow completed"
    );

    Ok(PurchaseOutcome {
        purchase_id: purchase.id,
        enrollment_id: enrollment.id,
        subscription_ids: subscriptions.iter().map(|s| s.id).collect(),
        entitlement_ids: entitlements.iter().map(|e| e.id).collect(),
        events_emitted: emitted.len(),
        ledger_entries: ledger.len(),
    })
}

/// Cross-check package items against their billing plans. Catches misconfigurations
/// (missing billing plans, mismatched counts, etc.) before any insert.
fn validate_package_items(package: &Package, billing_plans: &[BillingPlan]) -> AppResult<()> {
    if package.items.is_empty() {
        return Err(AppError::Validation("package has no items".into()));
    }
    if package.items.len() != billing_plans.len() {
        return Err(AppError::Validation(format!(
            "package has {} items but {} billing plans were loaded",
            package.items.len(),
            billing_plans.len()
        )));
    }
    for (idx, item) in package.items.iter().enumerate() {
        if item.quantity == 0 {
            return Err(AppError::Validation(format!(
                "package item {idx} has zero quantity"
            )));
        }
        let plan = &billing_plans[idx];
        if plan.catalog_item_id != item.catalog_item_id {
            return Err(AppError::Validation(format!(
                "package item {idx} billing plan {} references catalog item {} but item is for {}",
                plan.id, plan.catalog_item_id, item.catalog_item_id
            )));
        }
        if !plan.active {
            return Err(AppError::Validation(format!(
                "package item {idx} references inactive billing plan {}",
                plan.id
            )));
        }
        if plan.price.amount.is_sign_negative() {
            return Err(AppError::Validation(format!(
                "package item {idx} has negative price"
            )));
        }
    }
    Ok(())
}

/// Compute the authoritative package price from its billing plans. Each plan's
/// price is multiplied by the corresponding item's `quantity` and summed. All
/// plans must share a currency; a mixed-currency package is rejected.
fn compute_package_price(package: &Package, billing_plans: &[BillingPlan]) -> AppResult<Money> {
    let currency = billing_plans
        .first()
        .map(|p| p.price.currency.clone())
        .ok_or_else(|| AppError::Validation("package has no billing plans".into()))?;

    let mut total = rust_decimal::Decimal::ZERO;
    for (item, plan) in package.items.iter().zip(billing_plans.iter()) {
        if plan.price.currency != currency {
            return Err(AppError::Validation(format!(
                "package mixes currencies ({} vs {}); all billing plans must share one currency",
                currency, plan.price.currency
            )));
        }
        total += plan.price.amount * rust_decimal::Decimal::from(item.quantity);
    }
    Ok(Money::new(total, currency))
}

async fn load_billing_plans(
    catalog: &dyn CatalogRepo,
    package: &Package,
    conn: &mut sqlx::PgConnection,
) -> AppResult<Vec<BillingPlan>> {
    let mut out = Vec::with_capacity(package.items.len());
    for item in &package.items {
        let plan = catalog
            .get_billing_plan(item.billing_plan_id, conn)
            .await?
            .ok_or_else(|| {
                AppError::NotFound(format!("billing plan {} not found", item.billing_plan_id))
            })?;
        out.push(plan);
    }
    Ok(out)
}

async fn run_stack(
    runner: &StackRunner,
    stack: &PayPlanStack,
    enrollment: &Enrollment,
    emitted: &mut Vec<DomainEvent>,
    ledger: &mut Vec<payplan_core::payplan::ledger::RewardLedgerEntry>,
    state_changes_out: &mut Vec<StateChange>,
    state_cache: &mut StateCache,
) -> AppResult<()> {
    const MAX_ITERATIONS: usize = 32;
    let mut processed = 0;
    let mut iterations = 0;
    while processed < emitted.len() {
        if iterations >= MAX_ITERATIONS {
            return Err(AppError::Conflict(format!(
                "cascade exceeded {MAX_ITERATIONS} iterations; aborting"
            )));
        }
        iterations += 1;
        let event = emitted[processed].clone();
        processed += 1;
        let ctx = ModuleContext::new(enrollment.package_id)
            .with_enrollment(enrollment.id)
            .with_event(event.clone());
        let result: CoreResult<StackRunResult> = runner.run(stack, &event, &ctx, state_cache);
        let result = result.map_err(AppError::from)?;
        for new_event in result.emitted_events {
            emitted.push(new_event);
        }
        ledger.extend(result.ledger_entries);
        state_changes_out.extend(result.state_changes);
    }
    Ok(())
}

fn domain_event(
    event_type: EventType,
    payload: serde_json::Value,
) -> DomainEvent {
    DomainEvent {
        id: payplan_core::shared::ids::EventId::new(),
        event_type,
        payload,
        created_at: Utc::now(),
    }
}

/// Dependencies required by `handle_purchase_package`.
pub struct PurchaseDeps<'a> {
    pub pool: &'a sqlx::PgPool,
    pub packages: &'a dyn PackageRepo,
    pub catalog: &'a dyn CatalogRepo,
    pub purchases: &'a dyn PurchaseRepo,
    pub subscriptions: &'a dyn SubscriptionRepo,
    pub entitlements: &'a dyn EntitlementRepo,
    pub enrollments: &'a dyn EnrollmentRepo,
    pub pay_plan_stacks: &'a dyn PayPlanStackRepo,
    pub allocations: &'a dyn AllocationRepo,
    pub events: &'a dyn EventStore,
    pub ledger: &'a dyn RewardLedgerStore,
    pub registry: std::sync::Arc<ModuleRegistry>,
    pub purchase_writer: Option<&'a dyn PurchaseWriter>,
    pub module_state_store: Option<&'a dyn crate::ports::ModuleStateStore>,
    pub projector: Option<&'a dyn crate::ports::ModuleProjector>,
    pub event_projector: Option<&'a dyn crate::ports::EventProjector>,
}

#[derive(Debug)]
pub struct PurchaseOutcome {
    pub purchase_id: PurchaseId,
    pub enrollment_id: EnrollmentId,
    pub subscription_ids: Vec<SubscriptionId>,
    pub entitlement_ids: Vec<payplan_core::shared::ids::EntitlementId>,
    pub events_emitted: usize,
    pub ledger_entries: usize,
}

// Suppress unused import noise for traits we don't directly reference.
#[allow(dead_code)]
const _BILLING_PLAN_ID_TYPE: Option<BillingPlanId> = None;
