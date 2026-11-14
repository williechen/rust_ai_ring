use axum::{
    Form, Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::post,
};
use serde::Deserialize;
use shared::auth::token_digest;
use sqlx::SqlitePool;

#[derive(Deserialize)]
pub struct LogoutForm {
    pub csrf: String,
}

pub fn routes(pool: SqlitePool) -> Router {
    Router::new()
        .route("/logout", post(logout))
        .with_state(pool)
}

pub async fn logout(
    State(pool): State<SqlitePool>,
    headers: HeaderMap,
    Form(form): Form<LogoutForm>,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) != Some("http://127.0.0.1:3000") {
        return Err(StatusCode::FORBIDDEN);
    }

    let token = crate::auth_session::session_cookie(&headers)?;

    if form.csrf != token_digest(&format!("logout:{token}")) {
        return Err(StatusCode::FORBIDDEN);
    }

    let revoked = chat_persistence::member_session::revoke(&pool, &token_digest(&token))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !revoked {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok((
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/"),
            (
                header::SET_COOKIE,
                "chat_session=; HttpOnly; SameSite=Lax; Path=/; Max-Age=0",
            ),
            (header::CACHE_CONTROL, "no-store"),
        ],
    )
        .into_response())
}
