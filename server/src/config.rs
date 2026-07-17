use std::{net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    /// Origin of the organizer dashboard; magic links point here.
    pub app_base_url: String,
    /// Origin of this server; used for public page and webhook URLs.
    pub public_base_url: String,
    /// SMTP connection URL. When absent, magic links are logged and surfaced
    /// to the login page instead of emailed (development mode).
    pub smtp_url: Option<String>,
    pub email_from: String,
    pub cookie_secure: bool,
    pub session_ttl_days: i64,
    pub login_token_ttl_minutes: i64,
    /// If set, the built dashboard SPA is served from this directory.
    pub dashboard_dist: Option<PathBuf>,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let bind_addr = var("BIND_ADDR").unwrap_or_else(|| "0.0.0.0:8080".into());
        Ok(Self {
            database_url: var("DATABASE_URL").context("DATABASE_URL is required")?,
            bind_addr: bind_addr
                .parse()
                .with_context(|| format!("invalid BIND_ADDR {bind_addr:?}"))?,
            app_base_url: var("APP_BASE_URL")
                .unwrap_or_else(|| "http://localhost:5173".into())
                .trim_end_matches('/')
                .to_string(),
            public_base_url: var("PUBLIC_BASE_URL")
                .unwrap_or_else(|| "http://localhost:8080".into())
                .trim_end_matches('/')
                .to_string(),
            smtp_url: var("SMTP_URL"),
            email_from: var("EMAIL_FROM")
                .unwrap_or_else(|| "106 Events <no-reply@106.events>".into()),
            cookie_secure: var("COOKIE_SECURE").is_some_and(|v| v == "true" || v == "1"),
            session_ttl_days: 30,
            login_token_ttl_minutes: 15,
            dashboard_dist: var("DASHBOARD_DIST").map(PathBuf::from),
        })
    }
}
