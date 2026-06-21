//! `payplan-server` — HTTP entry point.
//!
//! Reads configuration from environment, connects to Postgres, runs migrations,
//! composes the application context, and starts an axum HTTP server.
//!
//! Environment variables:
//! - `DATABASE_URL` (required): Postgres connection string
//! - `JWT_SECRET` (required in release, dev default `"dev-secret-change-me"`): HS256 signing secret
//! - `BIND_ADDR` (optional, default `0.0.0.0:3000`)
//! - `RUST_LOG` (optional): e.g. `info,payplan=debug,sqlx=warn`

use std::net::SocketAddr;

use anyhow::{Context, Result};
use payplan_infra::migrator;
use payplan_infra::postgres::{connect, PgConfig};
use payplan_web::routes::build_router;
use payplan_web::AppContext;
use tokio::net::TcpListener;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,payplan=debug,sqlx=warn")),
        )
        .init();

    let cfg = PgConfig::from_env().context("DATABASE_URL must be set")?;
    let pool = connect(&cfg).await.context("connect to Postgres")?;

    info!("running migrations");
    migrator::run(&pool).await.context("migrations failed")?;

    // JWT secret: required in release builds; dev fallback only compiled in debug.
    let jwt_secret = match std::env::var("JWT_SECRET") {
        Ok(s) if !s.is_empty() => s,
        _other => {
            #[cfg(not(debug_assertions))]
            {
                anyhow::bail!(
                    "JWT_SECRET must be set to a non-empty value in release builds"
                );
            }
            #[cfg(debug_assertions)]
            {
                warn!("JWT_SECRET unset — using insecure dev default; do NOT use in production");
                AppContext::dev_jwt_secret()
            }
        }
    };

    let ctx = AppContext::new(pool, jwt_secret);

    let bind: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:3000".into())
        .parse()
        .context("BIND_ADDR must be a valid socket address")?;

    let app = build_router(ctx);
    let listener = TcpListener::bind(bind).await.context("bind")?;
    info!(%bind, "payplan-server listening");
    axum::serve(listener, app).await.context("axum::serve")?;
    Ok(())
}
