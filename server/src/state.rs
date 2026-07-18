use std::sync::Arc;

use sqlx::PgPool;

use crate::{config::Config, mailer::Mailer, messenger::Messenger};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub mailer: Arc<Mailer>,
    pub messenger: Arc<Messenger>,
}
