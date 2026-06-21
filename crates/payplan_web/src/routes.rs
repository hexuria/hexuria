//! Axum router. Wired into a tokio server in `payplan_server`. The same set of
//! handler functions can later be wrapped behind Spin/Leptos server functions
//! without changes.
//!
//! Routes are split into four groups by auth requirement:
//! - **public**: health, login, refresh, self-service signup (role forced to User)
//! - **authenticated**: any logged-in user (logout, purchases, package listing)
//! - **company_admin**: CompanyAdmin or PlatformAdmin (company + catalog creation)
//! - **platform_admin**: PlatformAdmin only (scheduled job triggers)

use axum::http::Method;
use axum::middleware::from_fn_with_state;
use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::trace::TraceLayer;

use crate::context::AppContext;
use crate::handlers::{
    close_binary_cycles_handler, create_billing_plan_handler, create_catalog_item_handler,
    create_company_handler, create_package_handler, health_handler, list_packages_handler,
    login_handler, logout_handler, purchase_package_handler, refresh_handler,
    register_user_handler, run_renewals_handler, run_royal_pot_distribution_handler,
};
use crate::session::{require_authenticated, require_company_admin, require_platform_admin};

pub fn build_router(ctx: AppContext) -> Router {
    // Public: no auth.
    let public = Router::new()
        .route("/health", get(health_handler))
        .route("/api/auth/login", post(login_handler))
        .route("/api/auth/refresh", post(refresh_handler))
        // Self-service signup; role is forced to User server-side.
        .route("/api/users", post(register_user_handler));

    // Authenticated: any logged-in user. State is passed explicitly because
    // the middleware extracts it before the outer `.with_state(ctx)` is applied.
    let authenticated = Router::new()
        .route("/api/auth/logout", post(logout_handler))
        .route("/api/purchases", post(purchase_package_handler))
        .route("/api/packages", get(list_packages_handler))
        .route_layer(from_fn_with_state(ctx.clone(), require_authenticated));

    // CompanyAdmin+: company + catalog/billing/package creation.
    let company_admin = Router::new()
        .route("/api/companies", post(create_company_handler))
        .route("/api/catalog_items", post(create_catalog_item_handler))
        .route("/api/billing_plans", post(create_billing_plan_handler))
        .route("/api/packages", post(create_package_handler))
        .route_layer(from_fn_with_state(ctx.clone(), require_company_admin()));

    // PlatformAdmin: scheduled job triggers only.
    let platform_admin = Router::new()
        .route("/admin/jobs/renewals/run", post(run_renewals_handler))
        .route(
            "/admin/jobs/royal_pot_distribution/run",
            post(run_royal_pot_distribution_handler),
        )
        .route(
            "/admin/jobs/binary_cycle_close/run",
            post(close_binary_cycles_handler),
        )
        .route_layer(from_fn_with_state(ctx.clone(), require_platform_admin()));

    // CORS: restrictive defaults for a payments API. In production the allowed
    // origins should come from configuration; for now deny all cross-origin
    // requests (AllowOrigin::list with an empty iter) except when CORS_ORIGIN is
    // set, in which case that single origin is permitted.
    let cors = match std::env::var("CORS_ORIGIN") {
        Ok(origin) if !origin.is_empty() => CorsLayer::new()
            .allow_origin(AllowOrigin::exact(origin.parse().expect("valid CORS_ORIGIN header")))
            .allow_methods([Method::GET, Method::POST])
            .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]),
        _ => CorsLayer::new()
            .allow_origin(AllowOrigin::list(std::iter::empty::<axum::http::HeaderValue>()))
            .allow_methods([Method::GET, Method::POST])
            .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::AUTHORIZATION]),
    };

    Router::new()
        .merge(public)
        .merge(authenticated)
        .merge(company_admin)
        .merge(platform_admin)
        .with_state(ctx)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
