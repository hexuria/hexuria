//! HTTP-level end-to-end tests for the JWT auth layer (Track C).
//!
//! Exercises the full axum router: login → purchase → logout → access denied
//! on the now-revoked token; purchase-for-another-user → 403; missing token
//! → 401; non-admin on an admin job → 403; missing token on an admin job → 401.
//!
//! Gated behind the `integration` feature (needs Postgres). Run with:
//!   `DATABASE_URL=postgres://... cargo test -p payplan_web --features integration -- --include-ignored --test-threads=1`

#![cfg(feature = "integration")]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use http_body_util::BodyExt;
use payplan_app::ports::{PasswordPort, TokenKind, TokenService};
use payplan_core::shared::ids::{BillingPlanId, CatalogItemId, CompanyId, PackageId, UserId};
use payplan_infra::auth::{JwtService, PasswordService};
use payplan_infra::postgres::{connect, PgConfig};
use payplan_web::routes::build_router;
use payplan_web::AppContext;
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;

/// Test JWT secret. Fixed so the test can decode tokens issued by the
/// server's `JwtService` (and verify auth flows round-trip).
const JWT_SECRET: &str = "test-secret-very-long-and-secure";

async fn pool() -> PgPool {
    let cfg = PgConfig::from_env().expect("DATABASE_URL");
    let pool = connect(&cfg).await.expect("connect");
    payplan_infra::migrator::run(&pool)
        .await
        .expect("migrations");
    pool
}

/// Build an `AppContext` with a real pool + a fixed JWT secret. The context
/// owns Arc<dyn ...> references to every repo/port; tests must share it.
async fn ctx(pool: PgPool) -> AppContext {
    AppContext::new(pool, JWT_SECRET.into())
}

/// Truncate every table touched by these tests, including `revoked_jti`.
async fn truncate_all(pool: &PgPool) {
    sqlx::query(
        "TRUNCATE TABLE reward_ledger, event_log, entitlements, enrollments, purchases, \
         subscriptions, package_items, packages, pay_plan_stack_modules, pay_plan_stacks, \
         billing_plans, catalog_items, revoked_jti, users, companies \
         RESTART IDENTITY CASCADE",
    )
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a company, a regular `UserRole::User`, an active `Package` with one
/// catalog item + billing plan. Returns `(company_id, user_id, package_id)`.
/// The user's password is hashed via the real `PasswordService`.
async fn seed_user_with_package(
    pool: &PgPool,
    passwords: &dyn PasswordPort,
    email: &str,
    password: &str,
) -> (CompanyId, UserId, PackageId) {
    let company_id = CompanyId::new();
    let user_id = UserId::new();
    let catalog_item_id = CatalogItemId::new();
    let billing_plan_id = BillingPlanId::new();
    let package_id = PackageId::new();

    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, 'TestCo', $2)")
        .bind(company_id)
        .bind(format!("test-co-{}", uuid::Uuid::now_v7().simple()))
        .execute(pool)
        .await
        .unwrap();

    let hash = passwords.hash(password).await.expect("hash password");
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, role, company_id) \
         VALUES ($1, $2, $3, 'user', $4)",
    )
    .bind(user_id)
    .bind(email)
    .bind(&hash)
    .bind(company_id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO catalog_items (id, company_id, name, item_type, sku, status, metadata) \
         VALUES ($1, $2, 'Item', 'service', $3, 'active', '{}')",
    )
    .bind(catalog_item_id)
    .bind(company_id)
    .bind(format!("sku-{}", uuid::Uuid::now_v7().simple()))
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO billing_plans (id, catalog_item_id, billing_type, price_amount, currency, \
         recurrence_interval, active) \
         VALUES ($1, $2, 'one_time', 100, 'USD', NULL, true)",
    )
    .bind(billing_plan_id)
    .bind(catalog_item_id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO packages (id, company_id, name, status, metadata) \
         VALUES ($1, $2, 'Pkg', 'active', '{}')",
    )
    .bind(package_id)
    .bind(company_id)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO package_items (id, package_id, catalog_item_id, billing_plan_id, quantity, \
         item_role, is_commissionable, commissionable_volume, points_value) \
         VALUES ($1, $2, $3, $4, 1, 'included', true, 10, 10)",
    )
    .bind(uuid::Uuid::now_v7())
    .bind(package_id)
    .bind(catalog_item_id)
    .bind(billing_plan_id)
    .execute(pool)
    .await
    .unwrap();

    (company_id, user_id, package_id)
}

/// Seed a company plus a `company_admin` user under it. Returns
/// `(company_id, admin_user_id)`. Password is hashed via the real service.
async fn seed_company_admin(
    pool: &PgPool,
    passwords: &dyn PasswordPort,
    email: &str,
    password: &str,
    role: &str,
) -> (CompanyId, UserId) {
    let company_id = CompanyId::new();
    let user_id = UserId::new();
    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, 'AdminCo', $2)")
        .bind(company_id)
        .bind(format!("admin-co-{}", uuid::Uuid::now_v7().simple()))
        .execute(pool)
        .await
        .unwrap();
    let hash = passwords.hash(password).await.expect("hash password");
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, role, company_id) VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id)
    .bind(email)
    .bind(&hash)
    .bind(role)
    .bind(company_id)
    .execute(pool)
    .await
    .unwrap();
    (company_id, user_id)
}

/// Send a JSON request through the router. Returns (status, body JSON).
async fn request(
    app: axum::Router,
    method: Method,
    uri: &str,
    body: Option<&Value>,
    bearer: Option<&str>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = bearer {
        builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    let req = if let Some(b) = body {
        builder
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(b).unwrap()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let resp = app.oneshot(req).await.expect("oneshot");
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, json)
}

/// Login as `(email, password)` and return `(access_token, refresh_token, user_id)`.
async fn login(app: axum::Router, email: &str, password: &str) -> (String, String, UserId) {
    let (status, body) = request(
        app,
        Method::POST,
        "/api/auth/login",
        Some(&json!({"email": email, "password": password})),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "login should succeed: {body}");
    let access = body["access_token"]
        .as_str()
        .expect("access_token")
        .to_string();
    let refresh = body["refresh_token"]
        .as_str()
        .expect("refresh_token")
        .to_string();
    let user_id = UserId::from(
        body["user_id"]
            .as_str()
            .expect("user_id")
            .parse::<uuid::Uuid>()
            .expect("user_id parse"),
    );
    (access, refresh, user_id)
}

// =============================================================================
// Tests
// =============================================================================

#[tokio::test]
async fn login_then_purchase_then_logout_blocks_reuse() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let ctx = ctx(pool.clone()).await;
    let passwords: Arc<dyn PasswordPort> = ctx.passwords.clone();
    let (company_id, user_id, package_id) = seed_user_with_package(
        &pool,
        passwords.as_ref(),
        "alice@example.test",
        "correct horse battery staple",
    )
    .await;
    let app = build_router(ctx);

    // 1. Login → 200 + token pair.
    let (access, _refresh, _login_user_id) = login(
        app.clone(),
        "alice@example.test",
        "correct horse battery staple",
    )
    .await;
    assert_eq!(_login_user_id, user_id);

    // 2. Purchase with access token → 201.
    let body = json!({
        "company_id": company_id,
        "user_id": user_id,
        "package_id": package_id,
        "sponsor_user_id": null,
    });
    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/purchases",
        Some(&body),
        Some(&access),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "purchase should succeed while token is live: {body}"
    );
    assert!(body["purchase_id"].is_string());

    // 3. Logout → 204.
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/auth/logout",
        Some(&json!({"access_token": access.clone(), "refresh_token": null})),
        Some(&access),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // 4. Same access token is now revoked → 401.
    let body = json!({
        "company_id": company_id,
        "user_id": user_id,
        "package_id": package_id,
        "sponsor_user_id": null,
    });
    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/purchases",
        Some(&body),
        Some(&access),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "purchase with revoked token must 401: {body}"
    );
}

#[tokio::test]
async fn purchase_for_other_user_is_403_for_regular_user() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let ctx = ctx(pool.clone()).await;
    let passwords: Arc<dyn PasswordPort> = ctx.passwords.clone();
    let (company_id, user_id, package_id) =
        seed_user_with_package(&pool, passwords.as_ref(), "bob@example.test", "p4ssw0rd!").await;
    let app = build_router(ctx);

    // Login as bob.
    let (access, _, _) = login(app.clone(), "bob@example.test", "p4ssw0rd!").await;

    // Try to purchase for a DIFFERENT user_id (someone else's uuid).
    let other_user_id = UserId::new();
    let body = json!({
        "company_id": company_id,
        "user_id": other_user_id,
        "package_id": package_id,
        "sponsor_user_id": null,
    });
    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/purchases",
        Some(&body),
        Some(&access),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "regular user cannot purchase for another user: {body}"
    );

    // Sanity: purchasing for SELF succeeds (and increments the user_id check).
    let body = json!({
        "company_id": company_id,
        "user_id": user_id,
        "package_id": package_id,
        "sponsor_user_id": null,
    });
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/purchases",
        Some(&body),
        Some(&access),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "self-purchase must succeed for a regular user"
    );
}

#[tokio::test]
async fn missing_token_is_401() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let ctx = ctx(pool.clone()).await;
    let passwords: Arc<dyn PasswordPort> = ctx.passwords.clone();
    let (company_id, user_id, package_id) =
        seed_user_with_package(&pool, passwords.as_ref(), "carol@example.test", "secret123").await;
    let app = build_router(ctx);

    let body = json!({
        "company_id": company_id,
        "user_id": user_id,
        "package_id": package_id,
        "sponsor_user_id": null,
    });
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/purchases",
        Some(&body),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "no Authorization header → 401"
    );
}

#[tokio::test]
async fn admin_job_requires_platform_admin() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let ctx = ctx(pool.clone()).await;
    let passwords: Arc<dyn PasswordPort> = ctx.passwords.clone();
    let (_company_id, user_id, _package_id) = seed_user_with_package(
        &pool,
        passwords.as_ref(),
        "dave@example.test",
        "very-secret",
    )
    .await;
    let app = build_router(ctx);

    // Login as the regular user.
    let (access, _, _) = login(app.clone(), "dave@example.test", "very-secret").await;
    let _ = user_id;

    // No token → 401.
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/admin/jobs/renewals/run",
        None,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "missing token on admin route → 401"
    );

    // Regular user token → 403.
    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/admin/jobs/renewals/run",
        None,
        Some(&access),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "regular user on admin route → 403: {body}"
    );
}

#[tokio::test]
async fn invalid_signature_is_401() {
    // A token signed with the WRONG secret must not pass verification.
    let pool = pool().await;
    truncate_all(&pool).await;
    let ctx = ctx(pool.clone()).await;
    let passwords: Arc<dyn PasswordPort> = ctx.passwords.clone();
    let (company_id, user_id, package_id) =
        seed_user_with_package(&pool, passwords.as_ref(), "eve@example.test", "p4ss").await;
    let app = build_router(ctx);

    // Issue a token with a different secret than the one the server uses.
    let rogue = JwtService::new("rogue-secret-not-the-real-one");
    let claims = rogue
        .issue_access(user_id.0, Some(company_id.0), "user")
        .await
        .expect("issue");
    let bad_token = rogue.encode(&claims).await.expect("encode");

    let body = json!({
        "company_id": company_id,
        "user_id": user_id,
        "package_id": package_id,
        "sponsor_user_id": null,
    });
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/purchases",
        Some(&body),
        Some(&bad_token),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "token signed with wrong secret → 401"
    );
}

#[tokio::test]
async fn refresh_rotates_and_revokes_old_refresh_jti() {
    // After refresh, the OLD refresh token must be rejected.
    let pool = pool().await;
    truncate_all(&pool).await;
    let ctx = ctx(pool.clone()).await;
    let passwords: Arc<dyn PasswordPort> = ctx.passwords.clone();
    seed_user_with_package(&pool, passwords.as_ref(), "frank@example.test", "rotate-me").await;
    let app = build_router(ctx);

    let (_access, refresh, _) = login(app.clone(), "frank@example.test", "rotate-me").await;

    // First refresh succeeds.
    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/auth/refresh",
        Some(&json!({"refresh_token": refresh.clone()})),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "first refresh should succeed: {body}"
    );

    // Second refresh with the SAME old token must fail (single-use rotation).
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/auth/refresh",
        Some(&json!({"refresh_token": refresh})),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "reusing a rotated refresh token → 401"
    );

    // Sanity: `revoked_jti` row was inserted for the original refresh jti.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM revoked_jti")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(count >= 1, "revoked_jti should hold at least one row");
}

#[tokio::test]
async fn logout_revokes_both_jtis() {
    // Verify that the access token used at /api/auth/logout is the one that
    // lands in `revoked_jti`. Also check the handler does not 401 the caller
    // if the supplied `access_token` is the same one in the Authorization
    // header (since `AuthUser` already validated it).
    let pool = pool().await;
    truncate_all(&pool).await;
    let ctx = ctx(pool.clone()).await;
    let passwords: Arc<dyn PasswordPort> = ctx.passwords.clone();
    seed_user_with_package(&pool, passwords.as_ref(), "gina@example.test", "p4ss").await;
    let app = build_router(ctx);

    let (access, refresh, _) = login(app.clone(), "gina@example.test", "p4ss").await;

    // Logout, revoking both.
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/auth/logout",
        Some(&json!({"access_token": access.clone(), "refresh_token": refresh.clone()})),
        Some(&access),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // Both jtis should be in revoked_jti.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM revoked_jti")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "logout with both tokens should insert exactly 2 revoked rows"
    );

    // And the access token is rejected on a subsequent request.
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/auth/logout",
        Some(&json!({"access_token": access, "refresh_token": null})),
        Some(&access),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "second logout with revoked token → 401"
    );
}

#[tokio::test]
async fn company_admin_cannot_create_package_for_another_company() {
    // Task 6: a company-A admin passing company_id = B must be rejected (403);
    // a platform admin may target any company.
    let pool = pool().await;
    truncate_all(&pool).await;
    let ctx = ctx(pool.clone()).await;
    let passwords: Arc<dyn PasswordPort> = ctx.passwords.clone();

    let (company_a, _admin_a) =
        seed_company_admin(&pool, passwords.as_ref(), "admin-a@example.test", "pw-a", "company_admin")
            .await;
    let company_b = CompanyId::new();
    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, 'CompanyB', $2)")
        .bind(company_b)
        .bind(format!("company-b-{}", uuid::Uuid::now_v7().simple()))
        .execute(&pool)
        .await
        .unwrap();
    let (_plat_co, _plat_admin) = seed_company_admin(
        &pool,
        passwords.as_ref(),
        "plat@example.test",
        "pw-plat",
        "platform_admin",
    )
    .await;

    let app = build_router(ctx);

    // Company-A admin tries to create a package under company B → 403.
    let (access_a, _, _) = login(app.clone(), "admin-a@example.test", "pw-a").await;
    let body = json!({
        "company_id": company_b,
        "name": "Cross-tenant pkg",
        "description": null,
        "pay_plan_stack_id": null,
        "items": [],
    });
    let (status, body_json) = request(
        app.clone(),
        Method::POST,
        "/api/packages",
        Some(&body),
        Some(&access_a),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "company-A admin must not create a package under company B: {body_json}"
    );

    // Company-A admin creating under their OWN company is allowed past the IDOR
    // gate (empty items then fails validation with a 400, NOT a 403).
    let body_own = json!({
        "company_id": company_a,
        "name": "Own pkg",
        "description": null,
        "pay_plan_stack_id": null,
        "items": [],
    });
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/packages",
        Some(&body_own),
        Some(&access_a),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "own-company package passes the IDOR gate (empty items → 400 validation)"
    );

    // Platform admin may target company B → passes the IDOR gate (400 on items).
    let (access_p, _, _) = login(app.clone(), "plat@example.test", "pw-plat").await;
    let (status, _) = request(
        app.clone(),
        Method::POST,
        "/api/packages",
        Some(&json!({
            "company_id": company_b,
            "name": "Plat pkg",
            "description": null,
            "pay_plan_stack_id": null,
            "items": [],
        })),
        Some(&access_p),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "platform admin passes the IDOR gate for any company (empty items → 400)"
    );
}

#[tokio::test]
async fn refresh_reissues_role_from_db_not_stale_claims() {
    // Task 7: a user demoted in the DB must not keep their old role by rotating
    // refresh tokens. After demotion, the refreshed access token must carry the
    // NEW (lower) role.
    let pool = pool().await;
    truncate_all(&pool).await;
    let ctx = ctx(pool.clone()).await;
    let passwords: Arc<dyn PasswordPort> = ctx.passwords.clone();
    let (_company, admin_id) = seed_company_admin(
        &pool,
        passwords.as_ref(),
        "demote@example.test",
        "pw-demote",
        "company_admin",
    )
    .await;
    let app = build_router(ctx);

    // Login as company_admin → tokens reflect the elevated role.
    let (_access, refresh, _) = login(app.clone(), "demote@example.test", "pw-demote").await;

    // Demote the user in the DB.
    sqlx::query("UPDATE users SET role = 'user' WHERE id = $1")
        .bind(admin_id)
        .execute(&pool)
        .await
        .unwrap();

    // Rotate the refresh token.
    let (status, body) = request(
        app.clone(),
        Method::POST,
        "/api/auth/refresh",
        Some(&json!({ "refresh_token": refresh })),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "refresh should succeed: {body}");

    // The response role and the decoded access-token claim must both be the new
    // lower role, not the stale company_admin from the old token.
    assert_eq!(
        body["role"].as_str(),
        Some("user"),
        "refreshed pair must report the demoted role"
    );
    let new_access = body["access_token"].as_str().expect("access_token");
    let verifier = JwtService::new(JWT_SECRET);
    let claims = verifier
        .verify(new_access, TokenKind::Access)
        .expect("verify new access token");
    assert_eq!(
        claims.role, "user",
        "refreshed access token must carry the DB-sourced role, not stale claims"
    );
}

#[tokio::test]
async fn password_service_smoke() {
    // Belt-and-suspenders: the argon2 round-trip used by the helper actually
    // verifies the correct password. Catches regressions where the seeded
    // hash format silently changes.
    let svc = PasswordService::new();
    let hash = svc.hash_blocking("hello").expect("hash");
    assert!(svc.verify_blocking("hello", &hash).unwrap());
    assert!(!svc.verify_blocking("nope", &hash).unwrap());
}
