pub mod http;

use analytics_server::{IngestState, IngestTokens, password_login};
use std::sync::Arc;

pub fn app(pool: sqlx::PgPool, chat: &str, pet: &str) -> axum::Router {
    password_login::build(IngestState {
        pool,
        tokens: Arc::new(IngestTokens {
            chat: chat.into(),
            pet: pet.into(),
        }),
    })
}
