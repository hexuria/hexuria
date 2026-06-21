//! Postgres-backed module state store.
//!
//! Loads and persists per-(module, aggregate) state blobs. Used by
//! `handle_purchase_package` to seed the engine cascade with prior progress
//! and to persist changes back in the same transaction as the purchase.

use std::collections::HashMap;

use async_trait::async_trait;
use payplan_app::error::{AppError, AppResult};
use payplan_app::ports::{ModuleStateChange, ModuleStateStore};
use sqlx::{PgConnection, PgPool, Row};

pub struct PgModuleStateStore {
    pool: PgPool,
}

impl PgModuleStateStore {
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
impl ModuleStateStore for PgModuleStateStore {
    async fn load_for_aggregate(
        &self,
        aggregate_id: uuid::Uuid,
        conn: &mut PgConnection,
    ) -> AppResult<HashMap<(String, String), serde_json::Value>> {
        let rows = sqlx::query(
            r#"SELECT module_key, module_version, state FROM module_state WHERE aggregate_id = $1"#,
        )
        .bind(aggregate_id)
        .fetch_all(conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;

        let mut out = HashMap::new();
        for row in rows {
            let module_key: String = row
                .try_get("module_key")
                .map_err(|e| AppError::Infra(e.to_string()))?;
            let module_version: String = row
                .try_get("module_version")
                .map_err(|e| AppError::Infra(e.to_string()))?;
            let state: serde_json::Value = row
                .try_get("state")
                .map_err(|e| AppError::Infra(e.to_string()))?;
            out.insert((module_key, module_version), state);
        }
        Ok(out)
    }

    async fn save(&self, change: ModuleStateChange<'_>, conn: &mut PgConnection) -> AppResult<()> {
        sqlx::query(
            r#"INSERT INTO module_state (module_key, module_version, aggregate_id, state, updated_at)
               VALUES ($1, $2, $3, $4, NOW())
               ON CONFLICT (module_key, module_version, aggregate_id)
               DO UPDATE SET state = EXCLUDED.state, updated_at = NOW()"#,
        )
        .bind(change.module_key)
        .bind(change.module_version)
        .bind(change.aggregate_id)
        .bind(change.state.clone())
        .execute(conn)
        .await
        .map_err(|e| AppError::Infra(e.to_string()))?;
        Ok(())
    }
}
