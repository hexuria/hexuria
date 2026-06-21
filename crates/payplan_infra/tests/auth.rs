//! Integration tests for `PgRevokedJtiStore` (Track C).
//!
//! Verifies that `revoke` inserts a row, `is_revoked` reflects it, unknown
//! jtis report as not-revoked, and re-revoking the same jti is idempotent.
//!
//! The `JwtService` unit tests already cover encode/verify/kind/expiry in
//! `crates/payplan_infra/src/auth.rs`, so this file only exercises the
//! Postgres-backed revocation store.
//!
//! Gated behind the `integration` feature. Run with:
//!   `DATABASE_URL=postgres://... cargo test -p payplan_infra --features integration -- --include-ignored --test-threads=1`

#![cfg(feature = "integration")]

use chrono::{DateTime, Duration, Utc};
use payplan_app::ports::{RevokedJtiStore, TokenKind};
use payplan_core::shared::ids::{CompanyId, UserId};
use payplan_infra::auth::PgRevokedJtiStore;
use payplan_infra::migrator;
use payplan_infra::postgres::{connect, PgConfig};
use sqlx::PgPool;

async fn pool() -> PgPool {
    let cfg = PgConfig::from_env().expect("DATABASE_URL");
    let pool = connect(&cfg).await.expect("connect");
    migrator::run(&pool).await.expect("migrations");
    pool
}

/// Truncate every table touched by these tests, including `revoked_jti` and
/// its FK targets (`users`, `companies`). CASCADE ensures dependent rows go.
async fn truncate_all(pool: &PgPool) {
    sqlx::query("TRUNCATE TABLE revoked_jti, users, companies RESTART IDENTITY CASCADE")
        .execute(pool)
        .await
        .unwrap();
}

/// Seed a company + user so `revoked_jti.user_id` has a valid FK target.
/// Returns the new user id.
async fn seed_user(pool: &PgPool) -> (CompanyId, UserId) {
    let company_id = CompanyId::new();
    let user_id = UserId::new();
    let slug = format!("auth-{}", uuid::Uuid::now_v7().simple());
    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, 'T', $2)")
        .bind(company_id)
        .bind(&slug)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, role, company_id) VALUES ($1, $2, 'ph', 'user', $3)",
    )
    .bind(user_id)
    .bind(format!("u-{}@t.local", uuid::Uuid::now_v7().simple()))
    .bind(company_id)
    .execute(pool)
    .await
    .unwrap();
    (company_id, user_id)
}

#[tokio::test]
async fn revoked_jti_round_trip() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (_company_id, user_id) = seed_user(&pool).await;
    let store = PgRevokedJtiStore::new(pool.clone());

    let jti = uuid::Uuid::now_v7().to_string();
    let expires_at: DateTime<Utc> = Utc::now() + Duration::days(7);

    // Unknown jti is not revoked.
    let mut conn = pool.acquire().await.unwrap();
    assert!(
        !store.is_revoked(&jti, &mut conn).await.unwrap(),
        "unknown jti must not be revoked"
    );
    drop(conn);

    // Revoke it.
    let mut conn = pool.acquire().await.unwrap();
    store
        .revoke(&jti, user_id.0, TokenKind::Access, expires_at, &mut conn)
        .await
        .expect("revoke");
    drop(conn);

    // Now it is revoked.
    let mut conn = pool.acquire().await.unwrap();
    assert!(
        store.is_revoked(&jti, &mut conn).await.unwrap(),
        "revoked jti must report as revoked"
    );
    drop(conn);

    // A different jti is still not revoked.
    let other_jti = uuid::Uuid::now_v7().to_string();
    let mut conn = pool.acquire().await.unwrap();
    assert!(
        !store.is_revoked(&other_jti, &mut conn).await.unwrap(),
        "other jti must not be revoked"
    );
    drop(conn);

    // Re-revoking the same jti is idempotent (ON CONFLICT DO NOTHING).
    let mut conn = pool.acquire().await.unwrap();
    store
        .revoke(&jti, user_id.0, TokenKind::Access, expires_at, &mut conn)
        .await
        .expect("idempotent re-revoke");
    drop(conn);

    // Row count stays at 1.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM revoked_jti WHERE jti = $1")
        .bind(&jti)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "re-revoke did not duplicate the row");

    // And the row carries the expected token_type.
    let token_type: String =
        sqlx::query_scalar("SELECT token_type FROM revoked_jti WHERE jti = $1")
            .bind(&jti)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(token_type, "access");
}

#[tokio::test]
async fn revoked_jti_stores_refresh_kind() {
    let pool = pool().await;
    truncate_all(&pool).await;
    let (_company_id, user_id) = seed_user(&pool).await;
    let store = PgRevokedJtiStore::new(pool.clone());

    let jti = uuid::Uuid::now_v7().to_string();
    let expires_at: DateTime<Utc> = Utc::now() + Duration::days(7);

    let mut conn = pool.acquire().await.unwrap();
    store
        .revoke(&jti, user_id.0, TokenKind::Refresh, expires_at, &mut conn)
        .await
        .expect("revoke refresh");
    drop(conn);

    let token_type: String =
        sqlx::query_scalar("SELECT token_type FROM revoked_jti WHERE jti = $1")
            .bind(&jti)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(token_type, "refresh", "TokenKind::Refresh round-trips");

    let mut conn = pool.acquire().await.unwrap();
    assert!(
        store.is_revoked(&jti, &mut conn).await.unwrap(),
        "refresh jtis are revocable too"
    );
}

#[tokio::test]
async fn revoked_jti_persists_expires_at() {
    // Ensures the `expires_at` column receives the value passed by the caller
    // (purge jobs rely on this to garbage-collect rows past natural expiry).
    let pool = pool().await;
    truncate_all(&pool).await;
    let (_company_id, user_id) = seed_user(&pool).await;
    let store = PgRevokedJtiStore::new(pool.clone());

    let jti = uuid::Uuid::now_v7().to_string();
    let expires_at: DateTime<Utc> = Utc::now() + Duration::hours(1);

    let mut conn = pool.acquire().await.unwrap();
    store
        .revoke(&jti, user_id.0, TokenKind::Access, expires_at, &mut conn)
        .await
        .expect("revoke");
    drop(conn);

    let stored: DateTime<Utc> =
        sqlx::query_scalar("SELECT expires_at FROM revoked_jti WHERE jti = $1")
            .bind(&jti)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored.timestamp(),
        expires_at.timestamp(),
        "expires_at is persisted verbatim"
    );
}
