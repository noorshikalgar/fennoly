//! Fennoly server.

mod access;
mod auth;
mod background;
mod backup;
mod config;
mod error;
mod model;
mod routes;
mod secrets;
mod spaces;
mod state;
mod storage;

use std::time::Duration;

use anyhow::Context;
use axum::Router;
use axum::http::{HeaderName, HeaderValue};
use sqlx::postgres::PgPoolOptions;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::state::AppState;
use crate::storage::Storage;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    // In development builds, also read the repository's .env, so the server
    // works when started from another folder.
    if cfg!(debug_assertions) {
        let repo_env = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.env");
        let _ = dotenvy::from_path(repo_env);
    }
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "fennoly=info,tower_http=info".into()))
        .init();

    let cfg = Config::from_env()?;
    let db = PgPoolOptions::new()
        .max_connections(16)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&cfg.database_url)
        .await
        .context("could not connect to PostgreSQL (check FENNOLY_DATABASE_URL)")?;
    sqlx::migrate!("./migrations").run(&db).await.context("database migration failed")?;

    let storage = Storage::new(&cfg.data_dir).context("could not prepare the data folder")?;
    let secrets = secrets::Secrets::load_or_create(&storage.secrets_key())
        .context("could not read or create secrets.key in the data folder")?;
    let state = AppState::new(db, cfg.clone(), storage, secrets);
    tracing::info!(
        restic = state.engine.restic.is_some(),
        rclone = state.engine.rclone.is_some(),
        "backup engines"
    );
    // Before serving requests, so no new run is mistaken for an interrupted one.
    if let Err(e) = backup::recover_after_restart(&state).await {
        tracing::warn!(error = %e, "could not tidy up backups after the last shutdown");
    }
    backup::spawn_scheduler(state.clone());
    background::spawn(state.clone());

    let tools = fennoly_tools::capabilities();
    tracing::info!(pdf = tools.pdf, image = tools.image, "file tools available");

    let app = app(state, &cfg);
    let listener = tokio::net::TcpListener::bind(cfg.bind).await?;
    tracing::info!("Fennoly {} listening on http://{}", env!("CARGO_PKG_VERSION"), cfg.bind);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

fn app(state: AppState, cfg: &Config) -> Router {
    let mut app = Router::new().nest("/api", routes::api());
    if let Some(dir) = &cfg.web_dir {
        // The web app is a single-page app: unknown paths get index.html.
        let index = ServeFile::new(dir.join("index.html"));
        app = app.fallback_service(ServeDir::new(dir).fallback(index));
    }
    app.layer(SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    ))
    .layer(SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("same-origin"),
    ))
    .layer(SetResponseHeaderLayer::if_not_present(
        HeaderName::from_static("x-frame-options"),
        HeaderValue::from_static("SAMEORIGIN"),
    ))
    .layer(TraceLayer::new_for_http())
    .with_state(state)
}
