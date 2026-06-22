use axum::{
    extract::{Extension, Form, Request, State},
    http::{header, HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
    routing::post,
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use payplan_app::auth::{login, refresh_tokens, revoke_tokens, AuthDeps, LoginInput};
use payplan_app::commands::{
    handle_create_billing_plan, handle_create_catalog_item, handle_create_company,
    CreateBillingPlanCommand, CreateCatalogItemCommand, CreateCompanyCommand,
};
use payplan_core::platform::catalog::{
    BillingType, CatalogItemType, RecurrenceInterval, RecurringSettings,
};
use payplan_core::platform::user::UserRole;
use payplan_core::shared::{ids::CatalogItemId, money::Money};
use payplan_infra::operations::{close_binary_cycles, run_renewals, run_royal_pot_distribution};
use payplan_web::handlers::purchase_deps;
use payplan_web::session::authenticate_access_token;
use payplan_web::session::AuthUser;
use serde::Deserialize;

use crate::ServerState;

const ACCESS_COOKIE: &str = "payplan_access";
const REFRESH_COOKIE: &str = "payplan_refresh";

pub(crate) fn action_routes() -> Router<ServerState> {
    Router::new()
        .route("/login", post(login_action))
        .route("/logout", post(logout_action))
        .route("/companies", post(create_company_action))
        .route("/catalog", post(create_catalog_action))
        .route("/billing", post(create_billing_action))
        .route("/jobs/renewals", post(run_renewals_action))
        .route("/jobs/royal-pot", post(run_royal_pot_action))
        .route("/jobs/binary-cycle", post(close_binary_cycle_action))
}

pub(crate) async fn require_ui_auth(
    State(state): State<ServerState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    if !is_protected_ui_path(&path) {
        return next.run(request).await;
    }

    let jar = CookieJar::from_headers(request.headers());
    let Some(token) = jar.get(ACCESS_COOKIE).map(Cookie::value) else {
        return login_redirect(&path);
    };
    match authenticate_access_token(&state.app, token).await {
        Ok(auth) => {
            request.extensions_mut().insert(auth);
            next.run(request).await
        }
        Err(_) => {
            let Some(refresh) = jar.get(REFRESH_COOKIE).map(Cookie::value) else {
                return clear_session(jar, login_redirect(&path));
            };
            match refresh_tokens(&auth_deps(&state), refresh).await {
                Ok(pair) => {
                    request.extensions_mut().insert(AuthUser {
                        user_id: pair.user_id,
                        company_id: pair.company_id,
                        role: pair.role,
                    });
                    let response = next.run(request).await;
                    let jar = jar
                        .add(token_cookie(ACCESS_COOKIE, pair.access_token, 15 * 60))
                        .add(token_cookie(
                            REFRESH_COOKIE,
                            pair.refresh_token,
                            7 * 24 * 60 * 60,
                        ));
                    (jar, response).into_response()
                }
                Err(_) => clear_session(jar, login_redirect(&path)),
            }
        }
    }
}

#[derive(Deserialize)]
struct LoginForm {
    email: String,
    password: String,
    next: Option<String>,
}

async fn login_action(
    State(state): State<ServerState>,
    jar: CookieJar,
    headers: HeaderMap,
    Form(form): Form<LoginForm>,
) -> Response {
    if let Some(response) = invalid_origin_response(&headers) {
        return response;
    }
    let pair = login(
        &auth_deps(&state),
        LoginInput {
            email: form.email,
            password: form.password,
        },
    )
    .await;
    match pair {
        Ok(pair) => {
            let jar = jar
                .add(token_cookie(ACCESS_COOKIE, pair.access_token, 15 * 60))
                .add(token_cookie(
                    REFRESH_COOKIE,
                    pair.refresh_token,
                    7 * 24 * 60 * 60,
                ));
            let destination = safe_next(form.next.as_deref());
            (jar, Redirect::to(destination)).into_response()
        }
        Err(_) => Redirect::to("/login?error=invalid_credentials").into_response(),
    }
}

async fn logout_action(
    State(state): State<ServerState>,
    jar: CookieJar,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = invalid_origin_response(&headers) {
        return response;
    }
    let access = jar
        .get(ACCESS_COOKIE)
        .map(Cookie::value)
        .unwrap_or_default();
    let refresh = jar.get(REFRESH_COOKIE).map(Cookie::value);
    let _ = revoke_tokens(&auth_deps(&state), access, refresh).await;
    let jar = jar
        .remove(removal_cookie(ACCESS_COOKIE))
        .remove(removal_cookie(REFRESH_COOKIE));
    (jar, Redirect::to("/login")).into_response()
}

fn auth_deps(state: &ServerState) -> AuthDeps<'_> {
    AuthDeps {
        pool: &state.app.pool,
        users: state.app.users.as_ref(),
        passwords: state.app.passwords.as_ref(),
        tokens: state.app.tokens.as_ref(),
        revoked_jti: state.app.revoked_jti.as_ref(),
    }
}

fn token_cookie(name: &'static str, value: String, max_age_seconds: i64) -> Cookie<'static> {
    Cookie::build((name, value))
        .http_only(true)
        .secure(cookie_secure())
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::seconds(max_age_seconds))
        .build()
}

fn removal_cookie(name: &'static str) -> Cookie<'static> {
    Cookie::build((name, ""))
        .http_only(true)
        .secure(cookie_secure())
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::ZERO)
        .build()
}

fn cookie_secure() -> bool {
    std::env::var("COOKIE_SECURE")
        .map(|value| value != "false" && value != "0")
        .unwrap_or(!cfg!(debug_assertions))
}

fn safe_next(next: Option<&str>) -> &'static str {
    match next {
        Some("/") => "/",
        Some("/packages") => "/packages",
        Some("/companies") => "/companies",
        Some("/catalog") => "/catalog",
        Some("/billing") => "/billing",
        Some("/purchases") => "/purchases",
        Some("/users") => "/users",
        Some("/jobs") => "/jobs",
        _ => "/",
    }
}

fn login_redirect(path: &str) -> Response {
    let destination = match path {
        "/packages" => "/login?next=/packages",
        "/companies" => "/login?next=/companies",
        "/catalog" => "/login?next=/catalog",
        "/billing" => "/login?next=/billing",
        "/purchases" => "/login?next=/purchases",
        "/users" => "/login?next=/users",
        "/jobs" => "/login?next=/jobs",
        _ => "/login",
    };
    Redirect::to(destination).into_response()
}

fn is_protected_ui_path(path: &str) -> bool {
    matches!(
        path,
        "/" | "/packages"
            | "/companies"
            | "/catalog"
            | "/billing"
            | "/purchases"
            | "/users"
            | "/jobs"
            | "/logout"
            | "/jobs/renewals"
            | "/jobs/royal-pot"
            | "/jobs/binary-cycle"
    ) || path.starts_with("/_server/")
}

fn clear_session(jar: CookieJar, response: Response) -> Response {
    let jar = jar
        .remove(removal_cookie(ACCESS_COOKIE))
        .remove(removal_cookie(REFRESH_COOKIE));
    (jar, response).into_response()
}

fn invalid_origin_response(headers: &HeaderMap) -> Option<Response> {
    let expected = std::env::var("PUBLIC_ORIGIN").ok()?;
    let origin_matches = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        == Some(expected.as_str());
    let expected_host = expected
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or(expected.as_str())
        .split('/')
        .next()
        .unwrap_or_default();
    let host_matches = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        == Some(expected_host);
    if origin_matches || (headers.get(header::ORIGIN).is_none() && host_matches) {
        None
    } else {
        Some(
            (
                StatusCode::FORBIDDEN,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                "invalid request origin",
            )
                .into_response(),
        )
    }
}

async fn run_renewals_action(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthUser>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = invalid_origin_response(&headers) {
        return response;
    }
    run_job(&state, &auth, JobKind::Renewals).await
}

async fn run_royal_pot_action(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthUser>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = invalid_origin_response(&headers) {
        return response;
    }
    run_job(&state, &auth, JobKind::RoyalPot).await
}

async fn close_binary_cycle_action(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthUser>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = invalid_origin_response(&headers) {
        return response;
    }
    run_job(&state, &auth, JobKind::BinaryCycle).await
}

enum JobKind {
    Renewals,
    RoyalPot,
    BinaryCycle,
}

async fn run_job(state: &ServerState, auth: &AuthUser, kind: JobKind) -> Response {
    if auth.role != UserRole::PlatformAdmin {
        return StatusCode::FORBIDDEN.into_response();
    }
    let deps = purchase_deps(&state.app);
    let result = match kind {
        JobKind::Renewals => run_renewals(&state.app.pool, &deps).await,
        JobKind::RoyalPot => run_royal_pot_distribution(&state.app.pool, &deps).await,
        JobKind::BinaryCycle => close_binary_cycles(&state.app.pool, &deps).await,
    };
    match result {
        Ok(_) => Redirect::to("/jobs?status=completed").into_response(),
        Err(error) => {
            tracing::error!(%error, "UI operations job failed");
            Redirect::to("/jobs?error=job_failed").into_response()
        }
    }
}

#[derive(Deserialize)]
struct CompanyForm {
    name: String,
    slug: String,
}

async fn create_company_action(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthUser>,
    headers: HeaderMap,
    Form(form): Form<CompanyForm>,
) -> Response {
    if let Some(response) = invalid_origin_response(&headers) {
        return response;
    }
    if auth.role != UserRole::PlatformAdmin {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result = handle_create_company(
        CreateCompanyCommand {
            name: form.name,
            slug: form.slug,
        },
        state.app.companies.as_ref(),
        &state.app.pool,
    )
    .await;
    action_redirect(result, "/companies")
}

#[derive(Deserialize)]
struct CatalogForm {
    name: String,
    description: Option<String>,
    sku: Option<String>,
    item_type: String,
}

async fn create_catalog_action(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthUser>,
    headers: HeaderMap,
    Form(form): Form<CatalogForm>,
) -> Response {
    if let Some(response) = invalid_origin_response(&headers) {
        return response;
    }
    let Some(company_id) = auth.company_id else {
        return StatusCode::FORBIDDEN.into_response();
    };
    if !auth.role.can_admin_company() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let item_type = match form.item_type.as_str() {
        "product" => CatalogItemType::Product,
        "service" => CatalogItemType::Service,
        _ => return Redirect::to("/catalog?error=invalid_item_type").into_response(),
    };
    let result = handle_create_catalog_item(
        CreateCatalogItemCommand {
            company_id,
            name: form.name,
            description: form.description.filter(|value| !value.trim().is_empty()),
            sku: form.sku.filter(|value| !value.trim().is_empty()),
            item_type,
        },
        state.app.catalog.as_ref(),
        &state.app.pool,
    )
    .await;
    action_redirect(result, "/catalog")
}

#[derive(Deserialize)]
struct BillingForm {
    catalog_item_id: uuid::Uuid,
    billing_type: String,
    price_amount: rust_decimal::Decimal,
    currency: String,
    recurrence_interval: Option<String>,
}

async fn create_billing_action(
    State(state): State<ServerState>,
    Extension(auth): Extension<AuthUser>,
    headers: HeaderMap,
    Form(form): Form<BillingForm>,
) -> Response {
    if let Some(response) = invalid_origin_response(&headers) {
        return response;
    }
    if !auth.role.can_admin_company() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let catalog_item_id = CatalogItemId::from(form.catalog_item_id);
    let mut conn = match state.app.pool.acquire().await {
        Ok(conn) => conn,
        Err(error) => {
            tracing::error!(%error, "billing UI failed to acquire connection");
            return Redirect::to("/billing?error=internal").into_response();
        }
    };
    let item = match state.app.catalog.get_item(catalog_item_id, &mut conn).await {
        Ok(Some(item)) => item,
        Ok(None) => return Redirect::to("/billing?error=not_found").into_response(),
        Err(error) => {
            tracing::error!(%error, "billing UI failed to load catalog item");
            return Redirect::to("/billing?error=internal").into_response();
        }
    };
    if auth.role != UserRole::PlatformAdmin && Some(item.company_id) != auth.company_id {
        return StatusCode::FORBIDDEN.into_response();
    }
    drop(conn);
    let billing_type = match form.billing_type.as_str() {
        "one_time" => BillingType::OneTime,
        "recurring" => BillingType::Recurring,
        _ => return Redirect::to("/billing?error=invalid_type").into_response(),
    };
    let recurring = if billing_type == BillingType::Recurring {
        let interval = match form.recurrence_interval.as_deref() {
            Some("daily") => RecurrenceInterval::Daily,
            Some("weekly") => RecurrenceInterval::Weekly,
            Some("monthly") => RecurrenceInterval::Monthly,
            Some("quarterly") => RecurrenceInterval::Quarterly,
            Some("yearly") => RecurrenceInterval::Yearly,
            _ => return Redirect::to("/billing?error=invalid_interval").into_response(),
        };
        Some(RecurringSettings {
            interval,
            interval_count: 1,
            trial_days: 0,
            grace_period_days: 0,
        })
    } else {
        None
    };
    let result = handle_create_billing_plan(
        CreateBillingPlanCommand {
            catalog_item_id,
            billing_type,
            price: Money::new(form.price_amount, form.currency),
            recurring,
        },
        state.app.catalog.as_ref(),
        &state.app.pool,
    )
    .await;
    action_redirect(result, "/billing")
}

fn action_redirect<T>(result: Result<T, payplan_app::error::AppError>, path: &str) -> Response {
    match result {
        Ok(_) => Redirect::to(&format!("{path}?status=created")).into_response(),
        Err(error) => {
            tracing::warn!(%error, path, "UI action rejected");
            Redirect::to(&format!("{path}?error=validation")).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_protected_ui_path, safe_next, token_cookie, ACCESS_COOKIE};

    #[test]
    fn next_redirect_is_restricted_to_known_ui_routes() {
        assert_eq!(safe_next(Some("/packages")), "/packages");
        assert_eq!(safe_next(Some("https://example.com")), "/");
        assert_eq!(safe_next(Some("//example.com")), "/");
    }

    #[test]
    fn token_cookie_is_http_only_and_site_scoped() {
        let cookie = token_cookie(ACCESS_COOKIE, "secret".into(), 60);
        assert_eq!(cookie.path(), Some("/"));
        assert_eq!(cookie.http_only(), Some(true));
        assert_eq!(
            cookie.same_site(),
            Some(axum_extra::extract::cookie::SameSite::Lax)
        );
    }

    #[test]
    fn api_routes_are_not_ui_cookie_authenticated() {
        assert!(!is_protected_ui_path("/api/packages"));
    }

    #[test]
    fn ui_routes_require_cookie_authentication() {
        assert!(is_protected_ui_path("/packages"));
    }
}
