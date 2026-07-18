use std::sync::Arc;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;

use server::{config::Config, mailer::Mailer, messenger::Messenger, reminders, routes, state::AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "server=debug,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await
        .context("connecting to postgres")?;
    sqlx::migrate!()
        .run(&pool)
        .await
        .context("running migrations")?;

    server::routes::api::team::seed_admins(&pool, &config.admin_emails)
        .await
        .context("seeding admins")?;
    if config.admin_emails.is_empty() {
        tracing::warn!(
            "ADMIN_EMAILS not set: nobody can sign in until an admin is seeded. \
             Set ADMIN_EMAILS to a comma-separated list and restart."
        );
    }

    let mailer = Mailer::from_config(&config)?;
    let messenger = Arc::new(Messenger::from_config(&config));
    let bind_addr = config.bind_addr;
    let public_base_url = config.public_base_url.clone();
    let state = AppState {
        pool: pool.clone(),
        config: Arc::new(config),
        mailer: Arc::new(mailer),
        messenger: Arc::clone(&messenger),
    };

    reminders::spawn_worker(pool, messenger, public_base_url);

    let app = routes::router(state);
    let listener = tokio::net::TcpListener::bind(bind_addr).await?;
    tracing::info!("listening on {bind_addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
