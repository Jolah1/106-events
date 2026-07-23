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
    /// Staff who are admins from the first boot, seeded on startup. Everyone
    /// else is invited from inside the app — but somebody has to be able to
    /// sign in and do the inviting, and that can't come from the app itself.
    pub admin_emails: Vec<String>,
    /// Shared secret the inbound WhatsApp/SMS webhook requires. When unset the
    /// webhook is open (development only) and logs a warning; in production the
    /// provider adapter must present this.
    pub webhook_secret: Option<String>,
    /// If set, the built dashboard SPA is served from this directory.
    pub dashboard_dist: Option<PathBuf>,
    /// Whether a sign-in request may return the magic link in its own response.
    ///
    /// Without this, a deploy that simply forgot to configure SMTP would hand a
    /// working link to anyone who asked for one — full account takeover by
    /// omission. So it is off unless someone deliberately turns it on, and it
    /// only ever applies when there's no mailer to send through anyway.
    pub allow_dev_login: bool,
    /// A shared passphrase that unlocks the same in-response sign-in link for
    /// one request, without opening it to everyone the way ALLOW_DEV_LOGIN
    /// does. For a deployed site that has no email yet: staff type this on the
    /// login page and get their link; anyone without it sees the normal flow.
    /// Like the dev link itself, it only ever applies when there's no mailer.
    pub staff_access_code: Option<String>,
}

fn var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Where to listen.
///
/// Railway, Render, Heroku and friends hand the container a port in `PORT` and
/// route to whatever binds it — a fixed port means the health check knocks on a
/// door nobody is behind. So `PORT` wins when it's there, and it always binds
/// every interface, because binding loopback inside a container is unreachable
/// from outside it.
///
/// `BIND_ADDR` stays for hosts that don't set `PORT` (Fly) and for development,
/// where two copies on one machine need different ports.
fn resolve_bind_addr(port: Option<String>, bind_addr: Option<String>) -> Result<SocketAddr> {
    if let Some(port) = port {
        let port: u16 = port
            .trim()
            .parse()
            .with_context(|| format!("invalid PORT {port:?}"))?;
        return Ok(SocketAddr::from(([0, 0, 0, 0], port)));
    }
    let addr = bind_addr.unwrap_or_else(|| "0.0.0.0:8080".into());
    addr.parse()
        .with_context(|| format!("invalid BIND_ADDR {addr:?}"))
}

impl Config {
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            database_url: var("DATABASE_URL").context("DATABASE_URL is required")?,
            bind_addr: resolve_bind_addr(var("PORT"), var("BIND_ADDR"))?,
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
            admin_emails: var("ADMIN_EMAILS")
                .unwrap_or_default()
                .split(',')
                .map(|e| e.trim().to_lowercase())
                .filter(|e| !e.is_empty())
                .collect(),
            webhook_secret: var("WEBHOOK_SECRET"),
            dashboard_dist: var("DASHBOARD_DIST").map(PathBuf::from),
            allow_dev_login: var("ALLOW_DEV_LOGIN")
                .is_some_and(|v| v == "true" || v == "1"),
            staff_access_code: var("STAFF_ACCESS_CODE"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_platform_supplied_port_wins() {
        // Railway hands the container a port and routes to it; ignoring that is
        // a health check knocking on the wrong door.
        let addr = resolve_bind_addr(Some("6543".into()), None).unwrap();
        assert_eq!(addr.to_string(), "0.0.0.0:6543");
    }

    #[test]
    fn a_platform_port_overrides_a_baked_in_bind_addr() {
        // The image ships BIND_ADDR=0.0.0.0:8080; PORT must still win, or the
        // container listens somewhere the platform isn't routing to.
        let addr = resolve_bind_addr(Some("6543".into()), Some("0.0.0.0:8080".into())).unwrap();
        assert_eq!(addr.to_string(), "0.0.0.0:6543");
    }

    #[test]
    fn without_a_platform_port_bind_addr_is_honoured() {
        let addr = resolve_bind_addr(None, Some("127.0.0.1:8090".into())).unwrap();
        assert_eq!(addr.to_string(), "127.0.0.1:8090");
    }

    #[test]
    fn the_default_binds_every_interface() {
        // Not 127.0.0.1: inside a container that's unreachable from outside.
        let addr = resolve_bind_addr(None, None).unwrap();
        assert_eq!(addr.to_string(), "0.0.0.0:8080");
    }

    #[test]
    fn a_nonsense_port_is_a_startup_error_not_a_silent_default() {
        assert!(resolve_bind_addr(Some("http".into()), None).is_err());
        assert!(resolve_bind_addr(None, Some("8080".into())).is_err());
    }
}
