use async_trait::async_trait;
use chrono::{DateTime, Utc};
use payplan_app::error::{AppError, AppResult};
use payplan_app::ports::{
    CatalogRepo, EnrollmentRepo, EntitlementRepo, PackageRepo, PayPlanStackRepo, PurchaseRepo,
    SubscriptionRepo,
};
use payplan_core::payplan::stack::{PayPlanStack, PayPlanStackStatus, StackModule};
use payplan_core::platform::catalog::{
    BillingPlan, BillingType, CatalogItem, CatalogItemStatus, CatalogItemType, RecurringSettings,
};
use payplan_core::platform::enrollment::{Enrollment, EnrollmentStatus};
use payplan_core::platform::entitlement::{Entitlement, EntitlementStatus};
use payplan_core::platform::package::{Package, PackageItem, PackageItemRole, PackageStatus};
use payplan_core::platform::purchase::{Purchase, PurchaseStatus};
use payplan_core::platform::subscription::{Subscription, SubscriptionStatus};
use payplan_core::shared::ids::{
    BillingPlanId, CatalogItemId, CompanyId, EnrollmentId, PackageId, PayPlanStackId, PurchaseId,
    SubscriptionId, UserId,
};
use serde_json::Value;
use sqlx::{PgConnection, PgPool, Row};

// ------------------------------- Catalog ------------------------------------

pub struct PgCatalogRepo {
    pool: PgPool,
}

impl PgCatalogRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl CatalogRepo for PgCatalogRepo {
    async fn insert_item(&self, item: &CatalogItem, conn: &mut PgConnection) -> AppResult<()> {
        let item_type = match item.item_type {
            CatalogItemType::Product => "product",
            CatalogItemType::Service => "service",
        };
        let status = catalog_item_status_str(item.status);
        sqlx::query(
            r#"INSERT INTO catalog_items (id, company_id, name, description, item_type, sku, status, metadata, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
        )
        .bind(item.id)
        .bind(item.company_id)
        .bind(&item.name)
        .bind(&item.description)
        .bind(item_type)
        .bind(&item.sku)
        .bind(status)
        .bind(&item.metadata)
        .bind(item.created_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get_item(&self, id: CatalogItemId, conn: &mut PgConnection) -> AppResult<Option<CatalogItem>> {
        let row = sqlx::query(
            r#"SELECT id, company_id, name, description, item_type, sku, status, metadata, created_at
               FROM catalog_items WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        row.map(row_to_catalog_item).transpose()
    }

    async fn list_items(&self, company_id: CompanyId, conn: &mut PgConnection) -> AppResult<Vec<CatalogItem>> {
        let rows = sqlx::query(
            r#"SELECT id, company_id, name, description, item_type, sku, status, metadata, created_at
               FROM catalog_items WHERE company_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(company_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        rows.into_iter().map(row_to_catalog_item).collect()
    }

    async fn insert_billing_plan(&self, plan: &BillingPlan, conn: &mut PgConnection) -> AppResult<()> {
        let billing_type = match plan.billing_type {
            BillingType::OneTime => "one_time",
            BillingType::Recurring => "recurring",
        };
        let (interval, count, trial, grace) = match &plan.recurring {
            Some(r) => (
                Some(interval_str(r.interval)),
                Some(i32::try_from(r.interval_count).unwrap_or(i32::MAX)),
                i32::try_from(r.trial_days).unwrap_or(0),
                i32::try_from(r.grace_period_days).unwrap_or(0),
            ),
            None => (None, None, 0, 0),
        };
        sqlx::query(
            r#"INSERT INTO billing_plans (id, catalog_item_id, billing_type, price_amount, currency, recurrence_interval, recurrence_count, trial_days, grace_period_days, active, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
        )
        .bind(plan.id)
        .bind(plan.catalog_item_id)
        .bind(billing_type)
        .bind(plan.price.amount)
        .bind(&plan.price.currency)
        .bind(interval)
        .bind(count)
        .bind(trial)
        .bind(grace)
        .bind(plan.active)
        .bind(plan.created_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get_billing_plan(&self, id: BillingPlanId, conn: &mut PgConnection) -> AppResult<Option<BillingPlan>> {
        let row = sqlx::query(
            r#"SELECT id, catalog_item_id, billing_type, price_amount, currency, recurrence_interval, recurrence_count, trial_days, grace_period_days, active, created_at
               FROM billing_plans WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        row.map(row_to_billing_plan).transpose()
    }
}

fn catalog_item_status_str(s: CatalogItemStatus) -> &'static str {
    match s {
        CatalogItemStatus::Active => "active",
        CatalogItemStatus::Inactive => "inactive",
        CatalogItemStatus::Archived => "archived",
    }
}

fn parse_recurring(
    s: String,
    count: Option<i32>,
    trial: i32,
    grace: i32,
) -> AppResult<RecurringSettings> {
    let interval = match s.as_str() {
        "daily" => payplan_core::platform::catalog::RecurrenceInterval::Daily,
        "weekly" => payplan_core::platform::catalog::RecurrenceInterval::Weekly,
        "monthly" => payplan_core::platform::catalog::RecurrenceInterval::Monthly,
        "quarterly" => payplan_core::platform::catalog::RecurrenceInterval::Quarterly,
        "yearly" => payplan_core::platform::catalog::RecurrenceInterval::Yearly,
        other => return Err(AppError::Validation(format!("unknown interval: {other}"))),
    };
    Ok(RecurringSettings {
        interval,
        interval_count: u32::try_from(count.unwrap_or(0)).unwrap_or(0),
        trial_days: u32::try_from(trial).unwrap_or(0),
        grace_period_days: u32::try_from(grace).unwrap_or(0),
    })
}

fn interval_str(i: payplan_core::platform::catalog::RecurrenceInterval) -> &'static str {
    use payplan_core::platform::catalog::RecurrenceInterval as R;
    match i {
        R::Daily => "daily",
        R::Weekly => "weekly",
        R::Monthly => "monthly",
        R::Quarterly => "quarterly",
        R::Yearly => "yearly",
    }
}

fn row_to_catalog_item(row: sqlx::postgres::PgRow) -> AppResult<CatalogItem> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let company_id: uuid::Uuid = row
        .try_get("company_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let name: String = row
        .try_get("name")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let description: Option<String> = row
        .try_get("description")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let item_type: String = row
        .try_get("item_type")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let sku: Option<String> = row
        .try_get("sku")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let metadata: Value = row
        .try_get("metadata")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(CatalogItem {
        id: CatalogItemId::from(id),
        company_id: CompanyId::from(company_id),
        name,
        description,
        item_type: match item_type.as_str() {
            "product" => CatalogItemType::Product,
            "service" => CatalogItemType::Service,
            other => return Err(AppError::Validation(format!("unknown item_type: {other}"))),
        },
        sku,
        status: match status.as_str() {
            "active" => CatalogItemStatus::Active,
            "inactive" => CatalogItemStatus::Inactive,
            "archived" => CatalogItemStatus::Archived,
            other => {
                return Err(AppError::Validation(format!(
                    "unknown item status: {other}"
                )))
            }
        },
        metadata,
        created_at,
    })
}

fn row_to_billing_plan(row: sqlx::postgres::PgRow) -> AppResult<BillingPlan> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let catalog_item_id: uuid::Uuid = row
        .try_get("catalog_item_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let billing_type: String = row
        .try_get("billing_type")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let price_amount: rust_decimal::Decimal = row
        .try_get("price_amount")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let currency: String = row
        .try_get("currency")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let interval: Option<String> = row
        .try_get("recurrence_interval")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let interval_count: Option<i32> = row
        .try_get("recurrence_count")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let trial_days: i32 = row
        .try_get("trial_days")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let grace_period_days: i32 = row
        .try_get("grace_period_days")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let active: bool = row
        .try_get("active")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let billing_type = match billing_type.as_str() {
        "one_time" => BillingType::OneTime,
        "recurring" => BillingType::Recurring,
        other => {
            return Err(AppError::Validation(format!(
                "unknown billing_type: {other}"
            )))
        }
    };
    let recurring = match interval {
        Some(s) => Some(parse_recurring(
            s,
            interval_count,
            trial_days,
            grace_period_days,
        )?),
        None => None,
    };
    Ok(BillingPlan {
        id: BillingPlanId::from(id),
        catalog_item_id: CatalogItemId::from(catalog_item_id),
        billing_type,
        price: payplan_core::shared::money::Money::new(price_amount, currency),
        recurring,
        active,
        created_at,
    })
}

// ------------------------------- Package ------------------------------------

pub struct PgPackageRepo {
    pool: PgPool,
}

impl PgPackageRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PackageRepo for PgPackageRepo {
    async fn insert(&self, package: &Package, conn: &mut PgConnection) -> AppResult<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Infra(e.to_string()))?;

        sqlx::query(
            r#"INSERT INTO packages (id, company_id, pay_plan_stack_id, name, description, status, metadata, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        )
        .bind(package.id)
        .bind(package.company_id)
        .bind(package.pay_plan_stack_id)
        .bind(&package.name)
        .bind(&package.description)
        .bind(package_status_str(package.status))
        .bind(&package.metadata)
        .bind(package.created_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;

        for item in &package.items {
            sqlx::query(
                r#"INSERT INTO package_items (id, package_id, catalog_item_id, billing_plan_id, quantity, item_role, is_commissionable, commissionable_volume, points_value)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"#,
            )
            .bind(uuid::Uuid::now_v7())
            .bind(package.id)
            .bind(item.catalog_item_id)
            .bind(item.billing_plan_id)
            .bind(i32::try_from(item.quantity).unwrap_or(i32::MAX))
            .bind(package_item_role_str(item.role))
            .bind(item.is_commissionable)
            .bind(i32::try_from(item.commissionable_volume).unwrap_or(i32::MAX))
            .bind(i32::try_from(item.points_value).unwrap_or(i32::MAX))
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Infra(e.to_string()))?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: PackageId, conn: &mut PgConnection) -> AppResult<Option<Package>> {
        let row = sqlx::query(
            r#"SELECT id, company_id, pay_plan_stack_id, name, description, status, metadata, created_at
               FROM packages WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let package = row_to_package(row)?;
        let items = sqlx::query(
            r#"SELECT catalog_item_id, billing_plan_id, quantity, item_role, is_commissionable, commissionable_volume, points_value
               FROM package_items WHERE package_id = $1"#,
        )
        .bind(id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        let items = items
            .into_iter()
            .map(row_to_package_item)
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Some(Package { items, ..package }))
    }

    async fn list(&self, company_id: CompanyId, conn: &mut PgConnection) -> AppResult<Vec<Package>> {
        let rows = sqlx::query(
            r#"SELECT id, company_id, pay_plan_stack_id, name, description, status, metadata, created_at
               FROM packages WHERE company_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(company_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        rows.into_iter().map(row_to_package).collect()
    }
}

fn package_status_str(s: PackageStatus) -> &'static str {
    match s {
        PackageStatus::Draft => "draft",
        PackageStatus::Active => "active",
        PackageStatus::Inactive => "inactive",
        PackageStatus::Archived => "archived",
    }
}

fn package_item_role_str(r: PackageItemRole) -> &'static str {
    match r {
        PackageItemRole::Included => "included",
        PackageItemRole::Required => "required",
        PackageItemRole::OptionalAddon => "optional_addon",
        PackageItemRole::Upsell => "upsell",
    }
}

fn row_to_package(row: sqlx::postgres::PgRow) -> AppResult<Package> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let company_id: uuid::Uuid = row
        .try_get("company_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let pay_plan_stack_id: Option<uuid::Uuid> = row
        .try_get("pay_plan_stack_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let name: String = row
        .try_get("name")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let description: Option<String> = row
        .try_get("description")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let metadata: Value = row
        .try_get("metadata")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(Package {
        id: PackageId::from(id),
        company_id: CompanyId::from(company_id),
        name,
        description,
        status: match status.as_str() {
            "draft" => PackageStatus::Draft,
            "active" => PackageStatus::Active,
            "inactive" => PackageStatus::Inactive,
            "archived" => PackageStatus::Archived,
            other => {
                return Err(AppError::Validation(format!(
                    "unknown package status: {other}"
                )))
            }
        },
        pay_plan_stack_id: pay_plan_stack_id.map(PayPlanStackId::from),
        default_billing_plan_id: None, // not stored in current schema; rebuilt on read
        metadata,
        created_at,
        items: vec![],
    })
}

fn row_to_package_item(row: sqlx::postgres::PgRow) -> AppResult<PackageItem> {
    let catalog_item_id: uuid::Uuid = row
        .try_get("catalog_item_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let billing_plan_id: uuid::Uuid = row
        .try_get("billing_plan_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let quantity: i32 = row
        .try_get("quantity")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let item_role: String = row
        .try_get("item_role")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let is_commissionable: bool = row
        .try_get("is_commissionable")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let commissionable_volume: i32 = row
        .try_get("commissionable_volume")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let points_value: i32 = row
        .try_get("points_value")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(PackageItem {
        catalog_item_id: CatalogItemId::from(catalog_item_id),
        billing_plan_id: BillingPlanId::from(billing_plan_id),
        quantity: u32::try_from(quantity).unwrap_or(0),
        role: match item_role.as_str() {
            "included" => PackageItemRole::Included,
            "required" => PackageItemRole::Required,
            "optional_addon" => PackageItemRole::OptionalAddon,
            "upsell" => PackageItemRole::Upsell,
            other => return Err(AppError::Validation(format!("unknown item_role: {other}"))),
        },
        is_commissionable,
        commissionable_volume: u32::try_from(commissionable_volume).unwrap_or(0),
        points_value: u32::try_from(points_value).unwrap_or(0),
    })
}

// ----------------------------- PayPlanStack ---------------------------------

pub struct PgPayPlanStackRepo {
    pool: PgPool,
}

impl PgPayPlanStackRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PayPlanStackRepo for PgPayPlanStackRepo {
    async fn insert(&self, stack: &PayPlanStack, conn: &mut PgConnection) -> AppResult<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Infra(e.to_string()))?;
        sqlx::query(
            r#"INSERT INTO pay_plan_stacks (id, company_id, name, version, status, created_at)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(stack.id)
        .bind(stack.company_id)
        .bind(&stack.name)
        .bind(i32::try_from(stack.version).unwrap_or(i32::MAX))
        .bind(stack_status_str(stack.status))
        .bind(stack.created_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;

        for m in &stack.modules {
            sqlx::query(
                r#"INSERT INTO pay_plan_stack_modules (id, stack_id, module_key, module_version, sort_order, config, active)
                   VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            )
            .bind(uuid::Uuid::now_v7())
            .bind(stack.id)
            .bind(&m.module_key)
            .bind(&m.module_version)
            .bind(i32::try_from(m.sort_order).unwrap_or(i32::MAX))
            .bind(&m.config)
            .bind(m.active)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Infra(e.to_string()))?;
        }
        tx.commit()
            .await
            .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: PayPlanStackId, conn: &mut PgConnection) -> AppResult<Option<PayPlanStack>> {
        let row = sqlx::query(
            r#"SELECT id, company_id, name, version, status, created_at
               FROM pay_plan_stacks WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let stack = row_to_stack(row)?;
        let modules = sqlx::query(
            r#"SELECT module_key, module_version, sort_order, config, active
               FROM pay_plan_stack_modules WHERE stack_id = $1 ORDER BY sort_order ASC"#,
        )
        .bind(id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        let modules = modules
            .into_iter()
            .map(row_to_stack_module)
            .collect::<AppResult<Vec<_>>>()?;
        Ok(Some(PayPlanStack { modules, ..stack }))
    }

    async fn next_version(&self, company_id: CompanyId, name: &str, conn: &mut PgConnection) -> AppResult<u32> {
        let row = sqlx::query(
            r#"SELECT COALESCE(MAX(version), 0) AS max_version
               FROM pay_plan_stacks WHERE company_id = $1 AND name = $2"#,
        )
        .bind(company_id)
        .bind(name)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        let max_version: i32 = row
            .try_get("max_version")
            .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(u32::try_from(max_version).unwrap_or(0) + 1)
    }
}

fn stack_status_str(s: PayPlanStackStatus) -> &'static str {
    match s {
        PayPlanStackStatus::Draft => "draft",
        PayPlanStackStatus::Active => "active",
        PayPlanStackStatus::Archived => "archived",
    }
}

fn row_to_stack(row: sqlx::postgres::PgRow) -> AppResult<PayPlanStack> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let company_id: uuid::Uuid = row
        .try_get("company_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let name: String = row
        .try_get("name")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let version: i32 = row
        .try_get("version")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(PayPlanStack {
        id: PayPlanStackId::from(id),
        company_id: CompanyId::from(company_id),
        name,
        version: u32::try_from(version).unwrap_or(0),
        status: match status.as_str() {
            "draft" => PayPlanStackStatus::Draft,
            "active" => PayPlanStackStatus::Active,
            "archived" => PayPlanStackStatus::Archived,
            other => {
                return Err(AppError::Validation(format!(
                    "unknown stack status: {other}"
                )))
            }
        },
        modules: vec![],
        created_at,
    })
}

fn row_to_stack_module(row: sqlx::postgres::PgRow) -> AppResult<StackModule> {
    let module_key: String = row
        .try_get("module_key")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let module_version: String = row
        .try_get("module_version")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let sort_order: i32 = row
        .try_get("sort_order")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let config: Value = row
        .try_get("config")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let active: bool = row
        .try_get("active")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(StackModule {
        module_key,
        module_version,
        sort_order: u32::try_from(sort_order).unwrap_or(0),
        config,
        active,
    })
}

// ------------------------------ Purchase ------------------------------------

pub struct PgPurchaseRepo {
    pool: PgPool,
}

impl PgPurchaseRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PurchaseRepo for PgPurchaseRepo {
    async fn insert(&self, purchase: &Purchase, conn: &mut PgConnection) -> AppResult<()> {
        let status = match purchase.status {
            PurchaseStatus::Pending => "pending",
            PurchaseStatus::Paid => "paid",
            PurchaseStatus::Failed => "failed",
            PurchaseStatus::Refunded => "refunded",
            PurchaseStatus::Cancelled => "cancelled",
        };
        sqlx::query(
            r#"INSERT INTO purchases (id, company_id, user_id, package_id, sponsor_user_id, gross_amount, net_amount, currency, status, purchased_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(purchase.id)
        .bind(purchase.company_id)
        .bind(purchase.user_id)
        .bind(purchase.package_id)
        .bind(purchase.sponsor_user_id)
        .bind(purchase.gross.amount)
        .bind(purchase.net.amount)
        .bind(&purchase.gross.currency)
        .bind(status)
        .bind(purchase.purchased_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: PurchaseId, conn: &mut PgConnection) -> AppResult<Option<Purchase>> {
        let row = sqlx::query(
            r#"SELECT id, company_id, user_id, package_id, sponsor_user_id, gross_amount, net_amount, currency, status, purchased_at
               FROM purchases WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        row.map(row_to_purchase).transpose()
    }
}

fn row_to_purchase(row: sqlx::postgres::PgRow) -> AppResult<Purchase> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let company_id: uuid::Uuid = row
        .try_get("company_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let user_id: uuid::Uuid = row
        .try_get("user_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let package_id: uuid::Uuid = row
        .try_get("package_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let sponsor_user_id: Option<uuid::Uuid> = row
        .try_get("sponsor_user_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let gross: rust_decimal::Decimal = row
        .try_get("gross_amount")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let net: rust_decimal::Decimal = row
        .try_get("net_amount")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let currency: String = row
        .try_get("currency")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let purchased_at: DateTime<Utc> = row
        .try_get("purchased_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(Purchase {
        id: PurchaseId::from(id),
        company_id: CompanyId::from(company_id),
        user_id: UserId::from(user_id),
        package_id: PackageId::from(package_id),
        sponsor_user_id: sponsor_user_id.map(UserId::from),
        gross: payplan_core::shared::money::Money::new(gross, currency.clone()),
        net: payplan_core::shared::money::Money::new(net, currency),
        status: match status.as_str() {
            "pending" => PurchaseStatus::Pending,
            "paid" => PurchaseStatus::Paid,
            "failed" => PurchaseStatus::Failed,
            "refunded" => PurchaseStatus::Refunded,
            "cancelled" => PurchaseStatus::Cancelled,
            other => {
                return Err(AppError::Validation(format!(
                    "unknown purchase status: {other}"
                )))
            }
        },
        purchased_at,
    })
}

// ----------------------------- Subscription ---------------------------------

pub struct PgSubscriptionRepo {
    pool: PgPool,
}

impl PgSubscriptionRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SubscriptionRepo for PgSubscriptionRepo {
    async fn insert(&self, sub: &Subscription, conn: &mut PgConnection) -> AppResult<()> {
        let status = match sub.status {
            SubscriptionStatus::Trialing => "trialing",
            SubscriptionStatus::Active => "active",
            SubscriptionStatus::PastDue => "past_due",
            SubscriptionStatus::Cancelled => "cancelled",
            SubscriptionStatus::Expired => "expired",
        };
        let (start, end) = sub
            .current_period
            .as_ref()
            .map_or((None, None), |p| (Some(p.starts_at), p.ends_at));
        sqlx::query(
            r#"INSERT INTO subscriptions (id, company_id, user_id, package_id, billing_plan_id, status, current_period_start, current_period_end, cancelled_at, created_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(sub.id)
        .bind(sub.company_id)
        .bind(sub.user_id)
        .bind(sub.package_id)
        .bind(sub.billing_plan_id)
        .bind(status)
        .bind(start)
        .bind(end)
        .bind(sub.cancelled_at)
        .bind(sub.created_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: SubscriptionId, conn: &mut PgConnection) -> AppResult<Option<Subscription>> {
        let row = sqlx::query(
            r#"SELECT id, company_id, user_id, package_id, billing_plan_id, status, current_period_start, current_period_end, cancelled_at, created_at
               FROM subscriptions WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        row.map(row_to_subscription).transpose()
    }

    async fn list_active_for_user(&self, user_id: UserId, conn: &mut PgConnection) -> AppResult<Vec<Subscription>> {
        let rows = sqlx::query(
            r#"SELECT id, company_id, user_id, package_id, billing_plan_id, status, current_period_start, current_period_end, cancelled_at, created_at
               FROM subscriptions WHERE user_id = $1 AND status = 'active'"#,
        )
        .bind(user_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        rows.into_iter().map(row_to_subscription).collect()
    }
}

fn row_to_subscription(row: sqlx::postgres::PgRow) -> AppResult<Subscription> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let company_id: uuid::Uuid = row
        .try_get("company_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let user_id: uuid::Uuid = row
        .try_get("user_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let package_id: uuid::Uuid = row
        .try_get("package_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let billing_plan_id: uuid::Uuid = row
        .try_get("billing_plan_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let start: Option<DateTime<Utc>> = row
        .try_get("current_period_start")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let end: Option<DateTime<Utc>> = row
        .try_get("current_period_end")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let cancelled_at: Option<DateTime<Utc>> = row
        .try_get("cancelled_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let created_at: DateTime<Utc> = row
        .try_get("created_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(Subscription {
        id: SubscriptionId::from(id),
        company_id: CompanyId::from(company_id),
        user_id: UserId::from(user_id),
        package_id: PackageId::from(package_id),
        billing_plan_id: BillingPlanId::from(billing_plan_id),
        status: match status.as_str() {
            "trialing" => SubscriptionStatus::Trialing,
            "active" => SubscriptionStatus::Active,
            "past_due" => SubscriptionStatus::PastDue,
            "cancelled" => SubscriptionStatus::Cancelled,
            "expired" => SubscriptionStatus::Expired,
            other => return Err(AppError::Validation(format!("unknown sub status: {other}"))),
        },
        current_period: start.map(|s| payplan_core::shared::period::Period {
            starts_at: s,
            ends_at: end,
        }),
        cancelled_at,
        created_at,
    })
}

// ------------------------------ Entitlement ---------------------------------

pub struct PgEntitlementRepo {
    pool: PgPool,
}

impl PgEntitlementRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EntitlementRepo for PgEntitlementRepo {
    async fn insert(&self, ent: &Entitlement, conn: &mut PgConnection) -> AppResult<()> {
        let status = match ent.status {
            EntitlementStatus::Active => "active",
            EntitlementStatus::Suspended => "suspended",
            EntitlementStatus::Expired => "expired",
            EntitlementStatus::Revoked => "revoked",
        };
        sqlx::query(
            r#"INSERT INTO entitlements (id, company_id, user_id, package_id, catalog_item_id, source_purchase_id, source_subscription_id, status, starts_at, ends_at, revoked_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
        )
        .bind(ent.id)
        .bind(ent.company_id)
        .bind(ent.user_id)
        .bind(ent.package_id)
        .bind(ent.catalog_item_id)
        .bind(ent.source_purchase_id)
        .bind(ent.source_subscription_id)
        .bind(status)
        .bind(ent.starts_at)
        .bind(ent.ends_at)
        .bind(ent.revoked_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn list_for_user(&self, user_id: UserId, conn: &mut PgConnection) -> AppResult<Vec<Entitlement>> {
        let rows = sqlx::query(
            r#"SELECT id, company_id, user_id, package_id, catalog_item_id, source_purchase_id, source_subscription_id, status, starts_at, ends_at, revoked_at
               FROM entitlements WHERE user_id = $1 ORDER BY starts_at DESC"#,
        )
        .bind(user_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        rows.into_iter().map(row_to_entitlement).collect()
    }
}

fn row_to_entitlement(row: sqlx::postgres::PgRow) -> AppResult<Entitlement> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let company_id: uuid::Uuid = row
        .try_get("company_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let user_id: uuid::Uuid = row
        .try_get("user_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let package_id: uuid::Uuid = row
        .try_get("package_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let catalog_item_id: uuid::Uuid = row
        .try_get("catalog_item_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let source_purchase_id: Option<uuid::Uuid> = row
        .try_get("source_purchase_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let source_subscription_id: Option<uuid::Uuid> = row
        .try_get("source_subscription_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let starts_at: DateTime<Utc> = row
        .try_get("starts_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let ends_at: Option<DateTime<Utc>> = row
        .try_get("ends_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let revoked_at: Option<DateTime<Utc>> = row
        .try_get("revoked_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(Entitlement {
        id: payplan_core::shared::ids::EntitlementId::from(id),
        company_id: CompanyId::from(company_id),
        user_id: UserId::from(user_id),
        package_id: PackageId::from(package_id),
        catalog_item_id: CatalogItemId::from(catalog_item_id),
        source_purchase_id: source_purchase_id.map(PurchaseId::from),
        source_subscription_id: source_subscription_id.map(SubscriptionId::from),
        status: match status.as_str() {
            "active" => EntitlementStatus::Active,
            "suspended" => EntitlementStatus::Suspended,
            "expired" => EntitlementStatus::Expired,
            "revoked" => EntitlementStatus::Revoked,
            other => {
                return Err(AppError::Validation(format!(
                    "unknown entitlement status: {other}"
                )))
            }
        },
        starts_at,
        ends_at,
        revoked_at,
    })
}

// ------------------------------ Enrollment ----------------------------------

pub struct PgEnrollmentRepo {
    pool: PgPool,
}

impl PgEnrollmentRepo {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl EnrollmentRepo for PgEnrollmentRepo {
    async fn insert(&self, enrollment: &Enrollment, conn: &mut PgConnection) -> AppResult<()> {
        let status = match enrollment.status {
            EnrollmentStatus::Active => "active",
            EnrollmentStatus::Suspended => "suspended",
            EnrollmentStatus::Cancelled => "cancelled",
            EnrollmentStatus::Expired => "expired",
        };
        sqlx::query(
            r#"INSERT INTO enrollments (id, company_id, user_id, package_id, purchase_id, sponsor_user_id, status, joined_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        )
        .bind(enrollment.id)
        .bind(enrollment.company_id)
        .bind(enrollment.user_id)
        .bind(enrollment.package_id)
        .bind(enrollment.purchase_id)
        .bind(enrollment.sponsor_user_id)
        .bind(status)
        .bind(enrollment.joined_at)
        .execute(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, id: EnrollmentId, conn: &mut PgConnection) -> AppResult<Option<Enrollment>> {
        let row = sqlx::query(
            r#"SELECT id, company_id, user_id, package_id, purchase_id, sponsor_user_id, status, joined_at
               FROM enrollments WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        row.map(row_to_enrollment).transpose()
    }

    async fn list_for_user(&self, user_id: UserId, conn: &mut PgConnection) -> AppResult<Vec<Enrollment>> {
        let rows = sqlx::query(
            r#"SELECT id, company_id, user_id, package_id, purchase_id, sponsor_user_id, status, joined_at
               FROM enrollments WHERE user_id = $1 ORDER BY joined_at DESC"#,
        )
        .bind(user_id)
        .fetch_all(&mut *conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        rows.into_iter().map(row_to_enrollment).collect()
    }
}

fn row_to_enrollment(row: sqlx::postgres::PgRow) -> AppResult<Enrollment> {
    let id: uuid::Uuid = row
        .try_get("id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let company_id: uuid::Uuid = row
        .try_get("company_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let user_id: uuid::Uuid = row
        .try_get("user_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let package_id: uuid::Uuid = row
        .try_get("package_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let purchase_id: uuid::Uuid = row
        .try_get("purchase_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let sponsor_user_id: Option<uuid::Uuid> = row
        .try_get("sponsor_user_id")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    let joined_at: DateTime<Utc> = row
        .try_get("joined_at")
        .map_err(|e| AppError::Infra(e.to_string()))?;
    Ok(Enrollment {
        id: EnrollmentId::from(id),
        company_id: CompanyId::from(company_id),
        user_id: UserId::from(user_id),
        package_id: PackageId::from(package_id),
        purchase_id: PurchaseId::from(purchase_id),
        sponsor_user_id: sponsor_user_id.map(UserId::from),
        status: match status.as_str() {
            "active" => EnrollmentStatus::Active,
            "suspended" => EnrollmentStatus::Suspended,
            "cancelled" => EnrollmentStatus::Cancelled,
            "expired" => EnrollmentStatus::Expired,
            other => {
                return Err(AppError::Validation(format!(
                    "unknown enrollment status: {other}"
                )))
            }
        },
        joined_at,
    })
}
