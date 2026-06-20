//! HTTP handlers. Each handler is a thin shim over `payplan_app` commands.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Json};
use axum::Form;
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

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(self)).into_response()
    }
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        Self {
            message: e.to_string(),
        }
    }
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
    Json(body): Json<CreateCatalogItemBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let item_type = match body.item_type.as_str() {
        "product" => CatalogItemType::Product,
        "service" => CatalogItemType::Service,
        other => {
            return Err(ApiError {
                message: format!("unknown item_type: {other}"),
            })
        }
    };
    let cmd = CreateCatalogItemCommand {
        company_id: CompanyId::from(body.company_id),
        name: body.name,
        description: body.description,
        sku: body.sku,
        item_type,
    };
    let item = handle_create_catalog_item(cmd, ctx.catalog.as_ref(), &ctx.pool)
        .await
        .map_err(ApiError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(&item).unwrap()),
    ))
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
    Ok((StatusCode::CREATED, Json(serde_json::to_value(&c).unwrap())))
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
    let mut value = serde_json::to_value(&u).unwrap();
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

#[instrument(skip(ctx, body))]
pub async fn create_billing_plan_handler(
    State(ctx): State<AppContext>,
    Json(body): Json<CreateBillingPlanBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let billing_type = match body.billing_type.as_str() {
        "one_time" => payplan_core::platform::catalog::BillingType::OneTime,
        "recurring" => payplan_core::platform::catalog::BillingType::Recurring,
        other => {
            return Err(ApiError {
                message: format!("unknown billing_type: {other}"),
            })
        }
    };
    let recurring = if billing_type == payplan_core::platform::catalog::BillingType::Recurring {
        let interval = body.recurring_interval.as_deref().ok_or_else(|| ApiError {
            message: "recurring billing requires recurring_interval".into(),
        })?;
        let interval_enum = match interval {
            "daily" => payplan_core::platform::catalog::RecurrenceInterval::Daily,
            "weekly" => payplan_core::platform::catalog::RecurrenceInterval::Weekly,
            "monthly" => payplan_core::platform::catalog::RecurrenceInterval::Monthly,
            "quarterly" => payplan_core::platform::catalog::RecurrenceInterval::Quarterly,
            "yearly" => payplan_core::platform::catalog::RecurrenceInterval::Yearly,
            other => {
                return Err(ApiError {
                    message: format!("unknown interval: {other}"),
                })
            }
        };
        let interval_count = body.recurring_interval_count.unwrap_or(1);
        if interval_count == 0 {
            return Err(ApiError {
                message: "recurring_interval_count must be > 0".into(),
            });
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
    let cmd = CreateBillingPlanCommand {
        catalog_item_id: payplan_core::shared::ids::CatalogItemId::from(body.catalog_item_id),
        billing_type,
        price: payplan_core::shared::money::Money::new(body.price_amount, body.currency),
        recurring,
    };
    let plan = handle_create_billing_plan(cmd, ctx.catalog.as_ref(), &ctx.pool)
        .await
        .map_err(ApiError::from)?;
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(&plan).unwrap()),
    ))
}

#[instrument(skip(ctx))]
pub async fn list_packages_handler(
    State(ctx): State<AppContext>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // For simplicity: list all packages across all companies. In real API,
    // extract company_id from auth context.
    let mut pool_conn = ctx.pool.acquire().await.map_err(|e| ApiError { message: e.to_string() })?;
    let companies = ctx.companies.list(pool_conn.as_mut()).await.map_err(ApiError::from)?;
    let mut all = Vec::new();
    for c in companies {
        let pkgs = ctx.packages.list(c.id, pool_conn.as_mut()).await.map_err(ApiError::from)?;
        all.extend(pkgs);
    }
    Ok(Json(serde_json::to_value(&all).unwrap()))
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

#[instrument(skip(ctx, body))]
pub async fn create_package_handler(
    State(ctx): State<AppContext>,
    Json(body): Json<CreatePackageBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiError> {
    let cmd = CreatePackageCommand {
        company_id: CompanyId::from(body.company_id),
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
    Ok((
        StatusCode::CREATED,
        Json(serde_json::to_value(&pkg).unwrap()),
    ))
}

#[derive(Debug, Deserialize)]
pub struct PurchaseBody {
    pub company_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub package_id: uuid::Uuid,
    pub sponsor_user_id: Option<uuid::Uuid>,
    pub payment_currency: String,
    pub gross_amount: rust_decimal::Decimal,
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
) -> Result<(StatusCode, Json<PurchaseResponse>), crate::session::AuthError> {
    // Purchase gate (Track C): regular users may only purchase for themselves;
    // admins (CompanyAdmin+) may initiate purchases on behalf of any user.
    if !auth.can_impersonate() && body.user_id != auth.user_id.0 {
        return Err(crate::session::AuthError::Forbidden);
    }
    let cmd = PurchasePackageCommand {
        company_id: CompanyId::from(body.company_id),
        user_id: UserId::from(body.user_id),
        package_id: PackageId::from(body.package_id),
        sponsor_user_id: body.sponsor_user_id.map(UserId::from),
        payment_currency: body.payment_currency,
        gross_amount: body.gross_amount,
    };
    let deps = build_purchase_deps(&ctx);
    let outcome = handle_purchase_package(cmd, &deps)
        .await
        .map_err(|e| crate::session::AuthError::InvalidToken(e.to_string()))?;
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

fn build_purchase_deps<'a>(ctx: &'a AppContext) -> PurchaseDeps<'a> {
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
    let deps = build_purchase_deps(&ctx);
    let processed = run_renewals(&ctx.pool, &deps)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(JobResult { processed }))
}

#[instrument(skip(ctx))]
pub async fn run_royal_pot_distribution_handler(
    State(ctx): State<AppContext>,
) -> Result<Json<JobResult>, ApiError> {
    let deps = build_purchase_deps(&ctx);
    let processed = run_royal_pot_distribution(&ctx.pool, &deps)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(JobResult { processed }))
}

#[instrument(skip(ctx))]
pub async fn close_binary_cycles_handler(
    State(ctx): State<AppContext>,
) -> Result<Json<JobResult>, ApiError> {
    let deps = build_purchase_deps(&ctx);
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
    let mut conn = ctx
        .pool
        .acquire()
        .await
        .map_err(|e| ApiError { message: e.to_string() })?;
    let user = ctx
        .users
        .find_by_email(&body.email, &mut conn)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError {
            message: "invalid credentials".into(),
        })?;
    // Constant-time-ish: treat "no such user" and "bad password" the same.
    let ok = ctx
        .passwords
        .verify(&body.password, &user.password_hash)
        .await
        .map_err(ApiError::from)?;
    if !ok {
        return Err(ApiError {
            message: "invalid credentials".into(),
        });
    }

    let role_str = user_role_str(user.role);
    let access = ctx
        .tokens
        .issue_access(user.id.0, user.company_id.map(|c| c.0), role_str)
        .await
        .map_err(ApiError::from)?;
    let refresh = ctx
        .tokens
        .issue_refresh(user.id.0, user.company_id.map(|c| c.0), role_str)
        .await
        .map_err(ApiError::from)?;
    let access_token = ctx.tokens.encode(&access).await.map_err(ApiError::from)?;
    let refresh_token = ctx.tokens.encode(&refresh).await.map_err(ApiError::from)?;
    Ok(Json(TokenPair {
        access_token,
        refresh_token,
        user_id: user.id.0,
        role: role_str.into(),
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
    let claims = ctx
        .tokens
        .verify(&body.refresh_token, payplan_app::ports::TokenKind::Refresh)
        .map_err(ApiError::from)?;

    // Single-use refresh rotation: revoke the presented refresh token's jti.
    let mut conn = ctx
        .pool
        .acquire()
        .await
        .map_err(|e| ApiError { message: e.to_string() })?;
    if ctx
        .revoked_jti
        .is_revoked(&claims.jti, &mut conn)
        .await
        .map_err(ApiError::from)?
    {
        return Err(ApiError {
            message: "refresh token revoked".into(),
        });
    }
    let exp = chrono::DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or(chrono::Utc::now());
    ctx.revoked_jti
        .revoke(&claims.jti, claims.sub, payplan_app::ports::TokenKind::Refresh, exp, &mut conn)
        .await
        .map_err(ApiError::from)?;

    // Issue a fresh pair.
    let access = ctx
        .tokens
        .issue_access(claims.sub, claims.company_id, &claims.role)
        .await
        .map_err(ApiError::from)?;
    let refresh = ctx
        .tokens
        .issue_refresh(claims.sub, claims.company_id, &claims.role)
        .await
        .map_err(ApiError::from)?;
    let access_token = ctx.tokens.encode(&access).await.map_err(ApiError::from)?;
    let refresh_token = ctx.tokens.encode(&refresh).await.map_err(ApiError::from)?;
    Ok(Json(TokenPair {
        access_token,
        refresh_token,
        user_id: claims.sub,
        role: claims.role,
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
) -> Result<StatusCode, crate::session::AuthError> {
    let mut conn = ctx
        .pool
        .acquire()
        .await
        .map_err(|e| crate::session::AuthError::InvalidToken(e.to_string()))?;

    // Revoke the access token. `AuthUser` already verified it, but we re-decode
    // to recover the jti/exp (AuthUser discards them).
    if let Ok(claims) = ctx
        .tokens
        .verify(&body.access_token, payplan_app::ports::TokenKind::Access)
    {
        let exp =
            chrono::DateTime::from_timestamp(claims.exp as i64, 0).unwrap_or(chrono::Utc::now());
        let _ = ctx
            .revoked_jti
            .revoke(&claims.jti, auth.user_id.0, payplan_app::ports::TokenKind::Access, exp, &mut conn)
            .await;
    }
    // Revoke the refresh token if supplied (best-effort; may be absent/expired).
    if let Some(refresh) = &body.refresh_token {
        if let Ok(claims) = ctx
            .tokens
            .verify(refresh, payplan_app::ports::TokenKind::Refresh)
        {
            let exp = chrono::DateTime::from_timestamp(claims.exp as i64, 0)
                .unwrap_or(chrono::Utc::now());
            let _ = ctx
                .revoked_jti
                .revoke(&claims.jti, auth.user_id.0, payplan_app::ports::TokenKind::Refresh, exp, &mut conn)
                .await;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

fn user_role_str(role: payplan_core::platform::user::UserRole) -> &'static str {
    use payplan_core::platform::user::UserRole;
    match role {
        UserRole::User => "user",
        UserRole::CompanyAdmin => "company_admin",
        UserRole::PlatformAdmin => "platform_admin",
    }
}
