//! Server configuration, read from `FENNOLY_*` environment variables
//! (and a `.env` file during development).

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub data_dir: PathBuf,
    pub bind: SocketAddr,
    /// Built web app to serve (`web/build`). When unset only the API is served.
    pub web_dir: Option<PathBuf>,
    /// Mark cookies `Secure`. Turn on when Fennoly is served over HTTPS.
    pub secure_cookies: bool,
    pub max_upload_bytes: u64,
    pub trash_days: i64,
    /// Command for database dumps (`FENNOLY_PG_DUMP`, default `pg_dump`).
    /// May include leading arguments, e.g. `docker exec -i db pg_dump`.
    pub pg_dump: Option<String>,
    /// Connection string pg_dump uses (`FENNOLY_PG_DUMP_URL`), when it
    /// differs from the server's own (for example inside a container).
    pub pg_dump_url: Option<String>,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = var("FENNOLY_DATABASE_URL")
            .unwrap_or_else(|| "postgres://fennoly:fennoly@localhost:5432/fennoly".into());
        let data_dir = PathBuf::from(var("FENNOLY_DATA_DIR").unwrap_or_else(|| "./data".into()));
        let mut bind: SocketAddr = var("FENNOLY_BIND")
            .unwrap_or_else(|| "0.0.0.0:8686".into())
            .parse()
            .context("FENNOLY_BIND must look like 0.0.0.0:8686")?;
        // Hosting tools often pass the port to listen on as PORT.
        if let Some(port) = var("PORT").and_then(|p| p.parse().ok()) {
            bind.set_port(port);
        }
        let web_dir = var("FENNOLY_WEB_DIR").map(PathBuf::from);
        let secure_cookies = var("FENNOLY_SECURE_COOKIES")
            .map(|v| matches!(v.as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);
        let max_upload_gb: u64 = var("FENNOLY_MAX_UPLOAD_GB")
            .map(|v| v.parse())
            .transpose()
            .context("FENNOLY_MAX_UPLOAD_GB must be a number")?
            .unwrap_or(50);
        let trash_days = var("FENNOLY_TRASH_DAYS")
            .map(|v| v.parse())
            .transpose()
            .context("FENNOLY_TRASH_DAYS must be a number")?
            .unwrap_or(30);
        Ok(Config {
            database_url,
            data_dir,
            bind,
            web_dir,
            secure_cookies,
            max_upload_bytes: max_upload_gb * 1024 * 1024 * 1024,
            trash_days,
            pg_dump: var("FENNOLY_PG_DUMP"),
            pg_dump_url: var("FENNOLY_PG_DUMP_URL"),
        })
    }
}
