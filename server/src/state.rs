use std::sync::Arc;

use sqlx::PgPool;

use crate::{config::Config, mailer::Mailer};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
    pub mailer: Arc<Mailer>,
}
