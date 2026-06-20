//! Integration tests for the Postgres-backed event store.
//!
//! These tests require a running Postgres. They are gated behind the
//! `integration` cfg so they don't break `cargo test` when no DB is available.
//!
//! Run with:
//!   `DATABASE_URL=postgres://... cargo test -p payplan_infra --features integration -- --include-ignored`

#![cfg(feature = "integration")]

use chrono::Utc;
use payplan_app::ports::EventStore;
use payplan_core::payplan::events::{DomainEvent, EventType};
use payplan_core::shared::ids::{CompanyId, EventId};
use payplan_infra::event_store::PgEventStore;
use payplan_infra::migrator;
use payplan_infra::postgres::{connect, PgConfig};
use serde_json::json;
use sqlx::PgPool;

async fn pool() -> PgPool {
    let cfg = PgConfig::from_env().expect("DATABASE_URL must be set");
    let pool = connect(&cfg).await.expect("connect");
    migrator::run(&pool).await.expect("migrations");
    pool
}

#[tokio::test]
async fn event_store_round_trip() {
    let pool = pool().await;
    // Seed a company so the FK constraint on event_log is satisfied.
    let company_id = CompanyId::new();
    sqlx::query("INSERT INTO companies (id, name, slug) VALUES ($1, 'T', $2)")
        .bind(company_id)
        .bind(format!("test-{}", uuid::Uuid::now_v7().simple()))
        .execute(&pool)
        .await
        .unwrap();

    let store = PgEventStore::new(pool.clone());

    let event = DomainEvent {
        id: EventId::new(),
        company_id: Some(company_id),
        event_type: EventType::PackagePurchased,
        payload: json!({"hello": "world"}),
        created_at: Utc::now(),
    };
    let mut conn = pool.acquire().await.unwrap();
    store.append(&[event], &mut conn).await.expect("append");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM event_log WHERE company_id = $1")
        .bind(company_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
