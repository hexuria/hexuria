//! Postgres-backed atomic purchase writer.
//!
//! Wraps the post-engine writes (subscriptions, entitlements, purchase,
//! enrollment, events, ledger, module state, projections) in a single Postgres
//! transaction. On any failure the transaction rolls back, so the engine
//! output and the domain rows either both persist or neither does.

use async_trait::async_trait;
use payplan_app::error::{AppError, AppResult};
use payplan_app::ports::{PurchaseWriter, PurchaseWrites};
use sqlx::PgPool;
use tracing::info;

pub struct PgPurchaseWriter {
    pool: PgPool,
}

impl PgPurchaseWriter {
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl PurchaseWriter for PgPurchaseWriter {
    async fn write(&self, writes: PurchaseWrites<'_>) -> AppResult<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| AppError::Infra(e.to_string()))?;

        // Subscriptions.
        for sub in writes.subscriptions {
            sqlx::query(
                r#"INSERT INTO subscriptions (id, company_id, user_id, package_id, billing_plan_id, status, current_period_start, current_period_end, cancelled_at, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
            )
            .bind(sub.id)
            .bind(sub.company_id)
            .bind(sub.user_id)
            .bind(sub.package_id)
            .bind(sub.billing_plan_id)
            .bind(subscription_status_str(sub.status))
            .bind(sub.current_period.as_ref().map(|p| p.starts_at))
            .bind(sub.current_period.as_ref().and_then(|p| p.ends_at))
            .bind(sub.cancelled_at)
            .bind(sub.created_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Infra(format!("insert subscription: {e}")))?;
        }

        // Entitlements.
        for ent in writes.entitlements {
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
            .bind(entitlement_status_str(ent.status))
            .bind(ent.starts_at)
            .bind(ent.ends_at)
            .bind(ent.revoked_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Infra(format!("insert entitlement: {e}")))?;
        }

        // Purchase.
        sqlx::query(
            r#"INSERT INTO purchases (id, company_id, user_id, package_id, sponsor_user_id, gross_amount, net_amount, currency, status, purchased_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)"#,
        )
        .bind(writes.purchase.id)
        .bind(writes.purchase.company_id)
        .bind(writes.purchase.user_id)
        .bind(writes.purchase.package_id)
        .bind(writes.purchase.sponsor_user_id)
        .bind(writes.purchase.gross.amount)
        .bind(writes.purchase.net.amount)
        .bind(&writes.purchase.gross.currency)
        .bind(purchase_status_str(writes.purchase.status))
        .bind(writes.purchase.purchased_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Infra(format!("insert purchase: {e}")))?;

        // Enrollment.
        sqlx::query(
            r#"INSERT INTO enrollments (id, company_id, user_id, package_id, purchase_id, sponsor_user_id, status, joined_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
        )
        .bind(writes.enrollment.id)
        .bind(writes.enrollment.company_id)
        .bind(writes.enrollment.user_id)
        .bind(writes.enrollment.package_id)
        .bind(writes.enrollment.purchase_id)
        .bind(writes.enrollment.sponsor_user_id)
        .bind(enrollment_status_str(writes.enrollment.status))
        .bind(writes.enrollment.joined_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| AppError::Infra(format!("insert enrollment: {e}")))?;

        // Events.
        for ev in writes.events {
            let aggregate_type = aggregate_type_for(ev.event_type);
            let aggregate_id = aggregate_id_for(ev);
            sqlx::query(
                r#"INSERT INTO event_log (id, company_id, event_type, aggregate_type, aggregate_id, payload, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            )
            .bind(ev.id)
            .bind(ev.company_id)
            .bind(ev.event_type.as_str())
            .bind(aggregate_type)
            .bind(aggregate_id)
            .bind(&ev.payload)
            .bind(ev.created_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Infra(format!("insert event: {e}")))?;
        }

        // Ledger.
        for entry in writes.ledger {
            sqlx::query(
                r#"INSERT INTO reward_ledger (id, company_id, user_id, enrollment_id, package_id, source_module, source_event_id, amount, points, currency, status, reason, created_at)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"#,
            )
            .bind(entry.id)
            .bind(entry.company_id)
            .bind(entry.user_id)
            .bind(entry.enrollment_id)
            .bind(entry.package_id)
            .bind(&entry.source_module)
            .bind(entry.source_event_id)
            .bind(entry.amount.amount)
            .bind(entry.points)
            .bind(&entry.amount.currency)
            .bind(ledger_status_str(entry.status))
            .bind(&entry.reason)
            .bind(entry.created_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Infra(format!("insert ledger entry: {e}")))?;
        }

        // Module state changes (per aggregate, opaque JSON).
        for change in writes.module_state_changes {
            sqlx::query(
                r#"INSERT INTO module_state (module_key, module_version, aggregate_id, state, updated_at)
                   VALUES ($1, $2, $3, $4, NOW())
                   ON CONFLICT (module_key, module_version, aggregate_id)
                   DO UPDATE SET state = EXCLUDED.state, updated_at = NOW()"#,
            )
            .bind(&change.module_key)
            .bind(&change.module_version)
            .bind(change.aggregate_id)
            .bind(&change.value)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::Infra(format!("upsert module_state: {e}")))?;
        }

        // Per-module projections (Track A2-A5).
        if let Some(projector) = writes.projector {
            projector
                .project(writes.module_state_changes, &mut tx)
                .await?;
        }
        // Event-driven projections (Track B1/B2): materialise rows that can't
        // be derived from module state (new enrollments, pairing results, ...).
        if let Some(event_projector) = writes.event_projector {
            event_projector.project(writes.events, &mut tx).await?;
        }

        tx.commit()
            .await
            .map_err(|e| AppError::Infra(format!("commit: {e}")))?;

        info!(
            subscriptions = writes.subscriptions.len(),
            entitlements = writes.entitlements.len(),
            events = writes.events.len(),
            ledger = writes.ledger.len(),
            module_state_changes = writes.module_state_changes.len(),
            "purchase writes committed atomically"
        );
        Ok(())
    }
}

fn subscription_status_str(
    s: payplan_core::platform::subscription::SubscriptionStatus,
) -> &'static str {
    use payplan_core::platform::subscription::SubscriptionStatus as S;
    match s {
        S::Trialing => "trialing",
        S::Active => "active",
        S::PastDue => "past_due",
        S::Cancelled => "cancelled",
        S::Expired => "expired",
    }
}

fn entitlement_status_str(
    s: payplan_core::platform::entitlement::EntitlementStatus,
) -> &'static str {
    use payplan_core::platform::entitlement::EntitlementStatus as E;
    match s {
        E::Active => "active",
        E::Suspended => "suspended",
        E::Expired => "expired",
        E::Revoked => "revoked",
    }
}

fn purchase_status_str(s: payplan_core::platform::purchase::PurchaseStatus) -> &'static str {
    use payplan_core::platform::purchase::PurchaseStatus as P;
    match s {
        P::Pending => "pending",
        P::Paid => "paid",
        P::Failed => "failed",
        P::Refunded => "refunded",
        P::Cancelled => "cancelled",
    }
}

fn enrollment_status_str(s: payplan_core::platform::enrollment::EnrollmentStatus) -> &'static str {
    use payplan_core::platform::enrollment::EnrollmentStatus as E;
    match s {
        E::Active => "active",
        E::Suspended => "suspended",
        E::Cancelled => "cancelled",
        E::Expired => "expired",
    }
}

fn ledger_status_str(s: payplan_core::payplan::ledger::LedgerStatus) -> &'static str {
    use payplan_core::payplan::ledger::LedgerStatus as L;
    match s {
        L::Pending => "pending",
        L::Approved => "approved",
        L::Paid => "paid",
        L::Reversed => "reversed",
        L::Voided => "voided",
    }
}

fn aggregate_type_for(event_type: payplan_core::payplan::events::EventType) -> &'static str {
    use payplan_core::payplan::events::EventType as E;
    match event_type {
        E::CompanyCreated => "company",
        E::UserCreated => "user",
        E::CatalogItemCreated => "catalog_item",
        E::BillingPlanCreated => "billing_plan",
        E::PackageCreated => "package",
        E::PackagePurchased => "purchase",
        E::SubscriptionCreated | E::SubscriptionRenewed | E::SubscriptionCancelled => {
            "subscription"
        }
        E::EntitlementGranted | E::EntitlementRevoked => "entitlement",
        E::EnrollmentCreated | E::EnrollmentSuspended | E::EnrollmentCancelled => "enrollment",
        E::RewardLedgerEntryCreated => "reward_ledger",
        _ => "module",
    }
}

fn aggregate_id_for(event: &payplan_core::payplan::events::DomainEvent) -> Option<uuid::Uuid> {
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
