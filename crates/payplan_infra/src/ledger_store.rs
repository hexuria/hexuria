//! Postgres-backed reward ledger store.

use async_trait::async_trait;
use payplan_app::error::{AppError, AppResult};
use payplan_app::ports::RewardLedgerStore;
use payplan_core::payplan::ledger::{LedgerStatus, RewardLedgerEntry};
use payplan_core::shared::ids::LedgerEntryId;
use sqlx::PgConnection;

#[derive(Default)]
pub struct PgLedgerStore {}

impl PgLedgerStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl RewardLedgerStore for PgLedgerStore {
    async fn append(
        &self,
        entries: &[RewardLedgerEntry],
        conn: &mut PgConnection,
    ) -> AppResult<Vec<LedgerEntryId>> {
        if entries.is_empty() {
            return Ok(vec![]);
        }

        let mut ids = Vec::with_capacity(entries.len());

        for entry in entries {
            let status = ledger_status_str(entry.status);
            sqlx::query(
                r#"
                INSERT INTO reward_ledger (
                    id, company_id, user_id, enrollment_id, package_id,
                    source_module, source_event_id, amount, points, currency,
                    status, reason, created_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                "#,
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
            .bind(status)
            .bind(&entry.reason)
            .bind(entry.created_at)
            .execute(&mut *conn)
            .await
            .map_err(|e| AppError::Infra(format!("insert ledger entry: {e}")))?;
            ids.push(entry.id);
        }

        Ok(ids)
    }
}

fn ledger_status_str(status: LedgerStatus) -> &'static str {
    match status {
        LedgerStatus::Pending => "pending",
        LedgerStatus::Approved => "approved",
        LedgerStatus::Paid => "paid",
        LedgerStatus::Reversed => "reversed",
        LedgerStatus::Voided => "voided",
    }
}
