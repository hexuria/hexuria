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
use axum::extract::FromRef;
use axum::Router;
use leptos::prelude::{get_configuration, LeptosOptions};
use leptos_axum::{generate_route_list, LeptosRoutes};
use payplan_infra::migrator;
use payplan_infra::postgres::{connect, PgConfig};
use payplan_ui::app::App;
use payplan_ui::shell::shell;
use payplan_web::routes::build_router;
use payplan_web::AppContext;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod ui_http;

#[derive(Clone)]
pub(crate) struct ServerState {
    pub(crate) app: AppContext,
    pub(crate) leptos: LeptosOptions,
}

impl FromRef<ServerState> for LeptosOptions {
    fn from_ref(state: &ServerState) -> Self {
        state.leptos.clone()
    }
}

impl FromRef<ServerState> for AppContext {
    fn from_ref(state: &ServerState) -> Self {
        state.app.clone()
    }
}

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
                anyhow::bail!("JWT_SECRET must be set to a non-empty value in release builds");
            }
            #[cfg(debug_assertions)]
            {
                tracing::warn!(
                    "JWT_SECRET unset — using insecure dev default; do NOT use in production"
                );
                AppContext::dev_jwt_secret()
            }
        }
    };

    let ctx = AppContext::new(pool, jwt_secret);

    let mut leptos_options = leptos_options().context("load Leptos configuration")?;
    if leptos_options.hash_files && !hash_file_exists(&leptos_options) {
        tracing::warn!(
            "hashed Leptos assets are unavailable for this executable; \
             falling back to unhashed development asset paths. Use `make serve` \
             to build and serve the browser assets."
        );
        leptos_options.hash_files = false;
    }
    let bind: SocketAddr = match std::env::var("BIND_ADDR") {
        Ok(value) => value
            .parse()
            .context("BIND_ADDR must be a valid socket address")?,
        Err(_) => leptos_options.site_addr,
    };

    let api_router = build_router(ctx.clone());
    let state = ServerState {
        app: ctx.clone(),
        leptos: leptos_options.clone(),
    };
    let routes = generate_route_list(App);
    let ui_context = {
        let ctx = ctx.clone();
        move || leptos::prelude::provide_context(ctx.clone())
    };
    let ui_router = Router::new()
        .leptos_routes_with_context(&state, routes, ui_context.clone(), {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler_with_context::<
            ServerState,
            _,
        >(ui_context, shell))
        .merge(ui_http::action_routes())
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            ui_http::require_ui_auth,
        ))
        .with_state(state);
    let app = api_router.merge(ui_router);
    let listener = TcpListener::bind(bind).await.context("bind")?;
    info!(%bind, "payplan-server listening");
    axum::serve(listener, app).await.context("axum::serve")?;
    Ok(())
}

fn hash_file_exists(options: &LeptosOptions) -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join(options.hash_file.as_ref()))
        })
        .is_some_and(|path| path.is_file())
}

fn leptos_options() -> Result<LeptosOptions> {
    if std::env::var_os("LEPTOS_OUTPUT_NAME").is_some() {
        Ok(get_configuration(None)?.leptos_options)
    } else {
        tracing::warn!(
            "cargo-leptos environment is unavailable; using development \
             defaults. Run `make serve` for frontend builds and live reload."
        );
        Ok(LeptosOptions::builder()
            .output_name("payplan-ui")
            .site_root("target/site")
            .site_pkg_dir("pkg")
            .server_fn_prefix("/_server".to_string())
            .build())
    }
}
