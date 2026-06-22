//! HTTP handlers. Each handler is a thin shim over `payplan_app` commands.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::Form;
use payplan_app::auth::{login, refresh_tokens, revoke_tokens, role_str, AuthDeps, LoginInput};
use payplan_app::commands::{
    default_module_registry, handle_create_billing_plan, handle_create_catalog_item,
    handle_create_company, handle_create_package, handle_purchase_package, handle_register_user,
    CreateBillingPlanCommand, CreateCatalogItemCommand, CreateCompanyCommand, CreatePackageCommand,
    PurchaseDeps, PurchasePackageCommand, RegisterUserCommand,
};
use payplan_app::error::AppError;
use payplan_core::platform::catalog::CatalogItemType;
use payplan_core::platform::package::PackageItem;
use payplan_core::shared::ids::{CompanyId, PackageId, UserId};
use payplan_infra::operations::{close_binary_cycles, run_renewals, run_royal_pot_distribution};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};

use crate::context::AppContext;

/// HTTP error with an explicit status. Domain/validation failures get a safe
/// 4xx with a client-facing message; infra/DB failures are collapsed to a
/// generic 500 so internal error strings (SQL, connection details) never reach
/// the client — the detail is logged server-side instead.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, message)
    }

    /// Collapse an internal error to a generic 500, logging the real detail.
    pub fn internal(context: &str, detail: impl std::fmt::Display) -> Self {
        tracing::error!(context, error = %detail, "internal error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal server error".to_string(),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ApiErrorBody {
                message: self.message,
            }),
        )
            .into_response()
    }
}

fn to_json<T: serde::Serialize>(val: &T) -> Result<serde_json::Value, ApiError> {
    serde_json::to_value(val).map_err(|e| ApiError::internal("serialize response", e))
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        match e {
            // Domain/validation errors carry a safe, client-facing message.
            AppError::Validation(_) | AppError::Core(_) => Self::bad_request(e.to_string()),
            AppError::NotFound(msg) => Self::new(StatusCode::NOT_FOUND, msg),
            AppError::Forbidden(msg) => Self::new(StatusCode::FORBIDDEN, msg),
            AppError::Conflict(msg) => Self::new(StatusCode::CONFLICT, msg),
            // Infra/unknown errors must NOT leak internal detail to the client.
            AppError::Infra(_) | AppError::Other(_) => Self::internal("app_error", e),
        }
    }
}

/// Resolve the company a write should target (IDOR guard — REMEDIATION_PLAN
/// Task 6). Company admins are pinned to their own company: a body-supplied
/// `company_id` that targets a DIFFERENT company is rejected with 403 so a
/// company-A admin can't create resources under company B. Only platform admins
/// may target an arbitrary company via the body.
fn effective_company(
    auth: &crate::session::AuthUser,
    body_company_id: uuid::Uuid,
) -> Result<CompanyId, ApiError> {
    if auth.can_admin_platform() {
        return Ok(CompanyId::from(body_company_id));
    }
    let own = auth
        .company_id
        .ok_or_else(|| ApiError::forbidden("caller is not scoped to a company"))?;
    if own.0 != body_company_id {
        return Err(ApiError::forbidden(
            "cannot create resources for another company",
        ));
    }
    Ok(own)
}

#[derive(Debug, Deserialize)]
pub struct CreateCatalogItemBody {
    pub company_id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub sku: Option<String>,
    pub item_type: String, // "product" | "service"
}

#[instrument(skip(ctx, body))]
pub async fn create_catalog_item_handler(
    State(ctx): State<AppContext>,
    auth: crate::session::AuthUser,
    Json(body): Json<CreateCatalogItemBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let item_type = match body.item_type.as_str() {
        "product" => CatalogItemType::Product,
        "service" => CatalogItemType::Service,
        other => return Err(ApiError::bad_request(format!("unknown item_type: {other}"))),
    };
    let company_id = effective_company(&auth, body.company_id)?;
    let cmd = CreateCatalogItemCommand {
        company_id,
        name: body.name,
        description: body.description,
        sku: body.sku,
        item_type,
    };
    let item = handle_create_catalog_item(cmd, ctx.catalog.as_ref(), &ctx.pool)
        .await
        .map_err(ApiError::from)?;
    Ok((StatusCode::CREATED, Json(to_json(&item)?)))
}

#[derive(Debug, Deserialize)]
pub struct CreateCompanyBody {
    pub name: String,
    pub slug: String,
}

#[instrument(skip(ctx, body))]
pub async fn create_company_handler(
    State(ctx): State<AppContext>,
    Json(body): Json<CreateCompanyBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let cmd = CreateCompanyCommand {
        name: body.name,
        slug: body.slug,
    };
    let c = handle_create_company(cmd, ctx.companies.as_ref(), &ctx.pool)
        .await
        .map_err(ApiError::from)?;
    Ok((StatusCode::CREATED, Json(to_json(&c)?)))
}

#[derive(Debug, Deserialize)]
pub struct RegisterUserBody {
    pub email: String,
    pub password: String,
    pub company_id: Option<uuid::Uuid>,
    // NOTE: `role` is intentionally absent on the public signup endpoint.
    // All users self-registering get `UserRole::User`; admin roles are only
    // assignable via seed data or a future admin-gated endpoint.
}

#[instrument(skip(ctx, body))]
pub async fn register_user_handler(
    State(ctx): State<AppContext>,
    Json(body): Json<RegisterUserBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let role = payplan_core::platform::user::UserRole::User;
    let cmd = RegisterUserCommand {
        email: body.email,
        password: body.password,
        role,
        company_id: body.company_id.map(CompanyId::from),
    };
    let u = handle_register_user(cmd, ctx.users.as_ref(), ctx.passwords.as_ref(), &ctx.pool)
        .await
        .map_err(ApiError::from)?;
    // Never leak the password hash in the response.
    let mut value = to_json(&u)?;
    if let Some(obj) = value.as_object_mut() {
        obj.remove("password_hash");
    }
    Ok((StatusCode::CREATED, Json(value)))
}

#[derive(Debug, Deserialize)]
pub struct CreateBillingPlanBody {
    pub catalog_item_id: uuid::Uuid,
    pub billing_type: String,
    pub price_amount: rust_decimal::Decimal,
    pub currency: String,
    pub recurring_interval: Option<String>,
    pub recurring_interval_count: Option<u32>,
    pub trial_days: Option<u32>,
    pub grace_period_days: Option<u32>,
}

#[instrument(skip(ctx, auth, body))]
pub async fn create_billing_plan_handler(
    State(ctx): State<AppContext>,
    auth: crate::session::AuthUser,
    Json(body): Json<CreateBillingPlanBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let billing_type = match body.billing_type.as_str() {
        "one_time" => payplan_core::platform::catalog::BillingType::OneTime,
        "recurring" => payplan_core::platform::catalog::BillingType::Recurring,
        other => {
            return Err(ApiError::bad_request(format!(
                "unknown billing_type: {other}"
            )))
        }
    };
    let recurring = if billing_type == payplan_core::platform::catalog::BillingType::Recurring {
        let interval = body.recurring_interval.as_deref().ok_or_else(|| {
            ApiError::bad_request("recurring billing requires recurring_interval")
        })?;
        let interval_enum = match interval {
            "daily" => payplan_core::platform::catalog::RecurrenceInterval::Daily,
            "weekly" => payplan_core::platform::catalog::RecurrenceInterval::Weekly,
            "monthly" => payplan_core::platform::catalog::RecurrenceInterval::Monthly,
            "quarterly" => payplan_core::platform::catalog::RecurrenceInterval::Quarterly,
            "yearly" => payplan_core::platform::catalog::RecurrenceInterval::Yearly,
            other => return Err(ApiError::bad_request(format!("unknown interval: {other}"))),
        };
        let interval_count = body.recurring_interval_count.unwrap_or(1);
        if interval_count == 0 {
            return Err(ApiError::bad_request(
                "recurring_interval_count must be > 0",
            ));
        }
        Some(payplan_core::platform::catalog::RecurringSettings {
            interval: interval_enum,
            interval_count,
            trial_days: body.trial_days.unwrap_or(0),
            grace_period_days: body.grace_period_days.unwrap_or(0),
        })
    } else {
        None
    };

    // IDOR guard (Task 6): a billing plan inherits its tenant from the catalog
    // item it references. Verify the referenced item belongs to the caller's
    // company (platform admins may target any item).
    let catalog_item_id = payplan_core::shared::ids::CatalogItemId::from(body.catalog_item_id);
    let mut conn = ctx
        .pool
        .acquire()
        .await
        .map_err(|e| ApiError::internal("acquire connection", e))?;
    let item = ctx
        .catalog
        .get_item(catalog_item_id, conn.as_mut())
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "catalog item not found"))?;
    drop(conn);
    if !auth.can_admin_platform() && Some(item.company_id) != auth.company_id {
        return Err(ApiError::forbidden(
            "catalog item belongs to a different company",
        ));
    }

    let cmd = CreateBillingPlanCommand {
        catalog_item_id,
        billing_type,
        price: payplan_core::shared::money::Money::new(body.price_amount, body.currency),
        recurring,
    };
    let plan = handle_create_billing_plan(cmd, ctx.catalog.as_ref(), &ctx.pool)
        .await
        .map_err(ApiError::from)?;
    Ok((StatusCode::CREATED, Json(to_json(&plan)?)))
}

#[instrument(skip(ctx))]
pub async fn list_packages_handler(
    State(ctx): State<AppContext>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // For simplicity: list all packages across all companies. In real API,
    // extract company_id from auth context.
    let mut pool_conn = ctx
        .pool
        .acquire()
        .await
        .map_err(|e| ApiError::internal("acquire connection", e))?;
    let companies = ctx
        .companies
        .list(pool_conn.as_mut())
        .await
        .map_err(ApiError::from)?;
    let mut all = Vec::new();
    for c in companies {
        let pkgs = ctx
            .packages
            .list(c.id, pool_conn.as_mut())
            .await
            .map_err(ApiError::from)?;
        all.extend(pkgs);
    }
    Ok(Json(to_json(&all)?))
}

#[derive(Debug, Deserialize)]
pub struct CreatePackageBody {
    pub company_id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub pay_plan_stack_id: Option<uuid::Uuid>,
    pub items: Vec<PackageItemBody>,
}

#[derive(Debug, Deserialize)]
pub struct PackageItemBody {
    pub catalog_item_id: uuid::Uuid,
    pub billing_plan_id: uuid::Uuid,
    pub quantity: u32,
    pub role: String,
    pub is_commissionable: bool,
    pub commissionable_volume: u32,
    pub points_value: u32,
}

impl From<PackageItemBody> for PackageItem {
    fn from(b: PackageItemBody) -> Self {
        PackageItem {
            catalog_item_id: payplan_core::shared::ids::CatalogItemId::from(b.catalog_item_id),
            billing_plan_id: payplan_core::shared::ids::BillingPlanId::from(b.billing_plan_id),
            quantity: b.quantity,
            role: match b.role.as_str() {
                "required" => payplan_core::platform::package::PackageItemRole::Required,
                "optional_addon" => payplan_core::platform::package::PackageItemRole::OptionalAddon,
                "upsell" => payplan_core::platform::package::PackageItemRole::Upsell,
                _ => payplan_core::platform::package::PackageItemRole::Included,
            },
            is_commissionable: b.is_commissionable,
            commissionable_volume: b.commissionable_volume,
            points_value: b.points_value,
        }
    }
}

#[instrument(skip(ctx, auth, body))]
pub async fn create_package_handler(
    State(ctx): State<AppContext>,
    auth: crate::session::AuthUser,
    Json(body): Json<CreatePackageBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let company_id = effective_company(&auth, body.company_id)?;
    let cmd = CreatePackageCommand {
        company_id,
        name: body.name,
        description: body.description,
        pay_plan_stack_id: body
            .pay_plan_stack_id
            .map(payplan_core::shared::ids::PayPlanStackId::from),
        items: body.items.into_iter().map(Into::into).collect(),
    };
    let pkg = handle_create_package(cmd, ctx.packages.as_ref(), &ctx.pool)
        .await
        .map_err(ApiError::from)?;
    Ok((StatusCode::CREATED, Json(to_json(&pkg)?)))
}

#[derive(Debug, Deserialize)]
pub struct PurchaseBody {
    pub company_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub package_id: uuid::Uuid,
    pub sponsor_user_id: Option<uuid::Uuid>,
}

#[derive(Debug, Serialize)]
pub struct PurchaseResponse {
    pub purchase_id: uuid::Uuid,
    pub enrollment_id: uuid::Uuid,
    pub subscription_ids: Vec<uuid::Uuid>,
    pub entitlement_ids: Vec<uuid::Uuid>,
    pub events_emitted: usize,
    pub ledger_entries: usize,
}

#[instrument(skip(ctx, auth, body))]
pub async fn purchase_package_handler(
    State(ctx): State<AppContext>,
    auth: crate::session::AuthUser,
    Json(body): Json<PurchaseBody>,
) -> Result<(StatusCode, Json<PurchaseResponse>), ApiError> {
    if !auth.can_impersonate() && body.user_id != auth.user_id.0 {
        return Err(ApiError::forbidden(
            "cannot purchase on behalf of another user",
        ));
    }
    let cmd = PurchasePackageCommand {
        company_id: CompanyId::from(body.company_id),
        user_id: UserId::from(body.user_id),
        package_id: PackageId::from(body.package_id),
        sponsor_user_id: body.sponsor_user_id.map(UserId::from),
    };
    let deps = purchase_deps(&ctx);
    let outcome = handle_purchase_package(cmd, &deps)
        .await
        .map_err(ApiError::from)?;
    info!(
        purchase_id = %outcome.purchase_id,
        "purchase flow completed via API"
    );
    Ok((
        StatusCode::CREATED,
        Json(PurchaseResponse {
            purchase_id: outcome.purchase_id.0,
            enrollment_id: outcome.enrollment_id.0,
            subscription_ids: outcome.subscription_ids.iter().map(|id| id.0).collect(),
            entitlement_ids: outcome.entitlement_ids.iter().map(|id| id.0).collect(),
            events_emitted: outcome.events_emitted,
            ledger_entries: outcome.ledger_entries,
        }),
    ))
}

#[instrument(skip(ctx))]
pub async fn health_handler(State(ctx): State<AppContext>) -> Json<serde_json::Value> {
    let ok = sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&ctx.pool)
        .await
        .is_ok();
    Json(serde_json::json!({
        "status": if ok { "ok" } else { "degraded" },
        "modules": ctx.registry.keys(),
    }))
}

// -------------------------------- Admin -------------------------------------

#[derive(Debug, Serialize)]
pub struct JobResult {
    pub processed: usize,
}

pub fn purchase_deps(ctx: &AppContext) -> PurchaseDeps<'_> {
    PurchaseDeps {
        pool: &ctx.pool,
        packages: ctx.packages.as_ref(),
        catalog: ctx.catalog.as_ref(),
        purchases: ctx.purchases.as_ref(),
        subscriptions: ctx.subscriptions.as_ref(),
        entitlements: ctx.entitlements.as_ref(),
        enrollments: ctx.enrollments.as_ref(),
        pay_plan_stacks: ctx.pay_plan_stacks.as_ref(),
        events: ctx.events.as_ref(),
        ledger: ctx.ledger.as_ref(),
        registry: ctx.registry.clone(),
        purchase_writer: Some(ctx.purchase_writer.as_ref()),
        module_state_store: Some(ctx.module_state_store.as_ref()),
        projector: Some(ctx.projector.as_ref()),
        event_projector: Some(ctx.event_projector.as_ref()),
    }
}

#[instrument(skip(ctx))]
pub async fn run_renewals_handler(
    State(ctx): State<AppContext>,
) -> Result<Json<JobResult>, ApiError> {
    let deps = purchase_deps(&ctx);
    let processed = run_renewals(&ctx.pool, &deps)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(JobResult { processed }))
}

#[instrument(skip(ctx))]
pub async fn run_royal_pot_distribution_handler(
    State(ctx): State<AppContext>,
) -> Result<Json<JobResult>, ApiError> {
    let deps = purchase_deps(&ctx);
    let processed = run_royal_pot_distribution(&ctx.pool, &deps)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(JobResult { processed }))
}

#[instrument(skip(ctx))]
pub async fn close_binary_cycles_handler(
    State(ctx): State<AppContext>,
) -> Result<Json<JobResult>, ApiError> {
    let deps = purchase_deps(&ctx);
    let processed = close_binary_cycles(&ctx.pool, &deps)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(JobResult { processed }))
}

// Re-export kept for symmetry with future Leptos server functions.
#[allow(dead_code)]
pub fn _module_registry_alias() -> payplan_core::payplan::registry::ModuleRegistry {
    default_module_registry()
}

// Suppress unused import noise for `Form` and `Path` (used in later phases).
#[allow(dead_code)]
fn _types(_f: Form<()>, _p: Path<()>) {}

// ===========================================================================
// Auth handlers (Track C)
// ===========================================================================

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub user_id: uuid::Uuid,
    pub role: String,
}

#[instrument(skip(ctx, body))]
pub async fn login_handler(
    State(ctx): State<AppContext>,
    Json(body): Json<LoginBody>,
) -> Result<Json<TokenPair>, ApiError> {
    let pair = login(
        &auth_deps(&ctx),
        LoginInput {
            email: body.email,
            password: body.password,
        },
    )
    .await
    .map_err(auth_api_error)?;
    Ok(Json(TokenPair {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        user_id: pair.user_id.0,
        role: role_str(pair.role).into(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct RefreshBody {
    pub refresh_token: String,
}

#[instrument(skip(ctx, body))]
pub async fn refresh_handler(
    State(ctx): State<AppContext>,
    Json(body): Json<RefreshBody>,
) -> Result<Json<TokenPair>, ApiError> {
    let pair = refresh_tokens(&auth_deps(&ctx), &body.refresh_token)
        .await
        .map_err(auth_api_error)?;
    Ok(Json(TokenPair {
        access_token: pair.access_token,
        refresh_token: pair.refresh_token,
        user_id: pair.user_id.0,
        role: role_str(pair.role).into(),
    }))
}

#[derive(Debug, Deserialize)]
pub struct LogoutBody {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

#[instrument(skip(ctx, body))]
pub async fn logout_handler(
    State(ctx): State<AppContext>,
    auth: crate::session::AuthUser,
    Json(body): Json<LogoutBody>,
) -> Result<StatusCode, ApiError> {
    let _ = auth;
    revoke_tokens(
        &auth_deps(&ctx),
        &body.access_token,
        body.refresh_token.as_deref(),
    )
    .await
    .map_err(ApiError::from)?;
    Ok(StatusCode::NO_CONTENT)
}

fn auth_deps(ctx: &AppContext) -> AuthDeps<'_> {
    AuthDeps {
        pool: &ctx.pool,
        users: ctx.users.as_ref(),
        passwords: ctx.passwords.as_ref(),
        tokens: ctx.tokens.as_ref(),
        revoked_jti: ctx.revoked_jti.as_ref(),
    }
}

fn auth_api_error(error: AppError) -> ApiError {
    match error {
        AppError::Forbidden(message) | AppError::NotFound(message) => {
            ApiError::new(StatusCode::UNAUTHORIZED, message)
        }
        other => ApiError::from(other),
    }
}
