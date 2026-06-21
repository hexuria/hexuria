//! Postgres-backed event store.

use async_trait::async_trait;
use payplan_app::error::{AppError, AppResult};
use payplan_app::ports::EventStore;
use payplan_core::payplan::events::{DomainEvent, EventType};
use sqlx::{PgConnection, PgPool};

pub struct PgEventStore {
    pool: PgPool,
}

impl PgEventStore {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[must_use]
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}

#[async_trait]
impl EventStore for PgEventStore {
    async fn append(&self, events: &[DomainEvent], conn: &mut PgConnection) -> AppResult<()> {
        if events.is_empty() {
            return Ok(());
        }

        for event in events {
            sqlx::query(
                r#"
                INSERT INTO event_log (id, company_id, event_type, aggregate_type, aggregate_id, payload, created_at)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
            )
            .bind(event.id)
            .bind(event.company_id)
            .bind(event.event_type.as_str())
            .bind(aggregate_type_for(event.event_type))
            .bind(aggregate_id_for(event))
            .bind(&event.payload)
            .bind(event.created_at)
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Infra(format!("insert event: {e}")))?;
        }
        Ok(())
    }
}

fn aggregate_type_for(event_type: EventType) -> &'static str {
    match event_type {
        EventType::CompanyCreated => "company",
        EventType::UserCreated => "user",
        EventType::CatalogItemCreated => "catalog_item",
        EventType::BillingPlanCreated => "billing_plan",
        EventType::PackageCreated => "package",
        EventType::PackagePurchased => "purchase",
        EventType::SubscriptionCreated
        | EventType::SubscriptionRenewed
        | EventType::SubscriptionCancelled => "subscription",
        EventType::EntitlementGranted | EventType::EntitlementRevoked => "entitlement",
        EventType::EnrollmentCreated
        | EventType::EnrollmentSuspended
        | EventType::EnrollmentCancelled => "enrollment",
        EventType::RewardLedgerEntryCreated => "reward_ledger",
        _ => "module",
    }
}

fn aggregate_id_for(event: &DomainEvent) -> Option<uuid::Uuid> {
    let extract = |key: &str| {
        event
            .payload
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
    };
    extract("aggregate_id")
        .or_else(|| extract("user_id"))
        .or_else(|| extract("package_id"))
        .or_else(|| extract("enrollment_id"))
        .or_else(|| extract("matrix_id"))
        .or_else(|| extract("royal_account_id"))
        .or_else(|| extract("node_id"))
}
