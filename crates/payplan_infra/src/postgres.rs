use std::time::Duration;

use sqlx::postgres::{PgConnectOptions, PgPool, PgPoolOptions};
use sqlx::ConnectOptions;

#[derive(Debug, Clone)]
pub struct PgConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout: Duration,
}

impl PgConfig {
    pub fn from_env() -> Result<Self, std::env::VarError> {
        let url = std::env::var("DATABASE_URL")?;
        Ok(Self::from_url(url))
    }

    pub fn from_url(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            max_connections: 16,
            min_connections: 1,
            connect_timeout: Duration::from_secs(5),
        }
    }

    pub fn connect_options(&self) -> PgConnectOptions {
        self.url
            .parse::<PgConnectOptions>()
            .expect("invalid DATABASE_URL")
            .log_statements(tracing::log::LevelFilter::Debug)
    }
}

pub async fn connect(cfg: &PgConfig) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .min_connections(cfg.min_connections)
        .acquire_timeout(cfg.connect_timeout)
        .connect_with(cfg.connect_options())
        .await
}

/// A lazily-connected pool useful for tests and Spin where you may want to defer the actual connect.
pub fn connect_lazy(cfg: &PgConfig) -> PgPool {
    PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .min_connections(cfg.min_connections)
        .acquire_timeout(cfg.connect_timeout)
        .connect_lazy_with(cfg.connect_options())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_url_sets_defaults() {
        let cfg = PgConfig::from_url("postgres://localhost/db");
        assert_eq!(cfg.max_connections, 16);
        assert_eq!(cfg.min_connections, 1);
        assert_eq!(cfg.url, "postgres://localhost/db");
    }

    #[tokio::test]
    async fn connect_lazy_does_not_block() {
        let cfg = PgConfig::from_url("postgres://127.0.0.1:1/no-such-db");
        let _pool: PgPool = connect_lazy(&cfg);
    }
}
