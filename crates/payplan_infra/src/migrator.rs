use sqlx::migrate::MigrateError;
use sqlx::PgPool;
use tracing::info;

/// Apply all embedded migrations in this crate.
///
/// The migrations live in `crates/payplan_infra/migrations/*.sql` and are compiled
/// in via `sqlx::migrate!`. This is idempotent.
pub async fn run(pool: &PgPool) -> Result<(), MigrateError> {
    info!("running database migrations");
    sqlx::migrate!("./migrations").run(pool).await?;
    info!("migrations applied");
    Ok(())
}
