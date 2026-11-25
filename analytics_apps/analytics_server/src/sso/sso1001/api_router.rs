use super::api_dto::{Login, LoginReturn};
use crate::{
    IngestState,
    config::auth::ORIGIN,
    password_login::{digest, renewed},
};
use analytics_application::login_attempt as attempts;
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    Form,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};

pub(crate) async fn login_page(Query(input): Query<LoginReturn>) -> Html<String> {
    Html(super::web::render_login_with_next(
        "",
        "",
        analytics_application::return_target::checked(&input.next).unwrap_or(""),
    ))
}

fn login_feedback(status: StatusCode, name: &str, wait: &str, next: &str) -> Response {
    let message = if wait == "0" {
        "帳號或密碼錯誤".to_owned()
    } else {
        format!("請等待 {wait} 秒後重試")
    };
    let mut res = (
        status,
        Html(super::web::render_login_with_next(name, &message, next)),
    )
        .into_response();
    if wait != "0" {
        if let Ok(value) = wait.parse() {
            res.headers_mut().insert(header::RETRY_AFTER, value);
        }
    }
    res
}

pub(crate) async fn login(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Form(form): Form<Login>,
) -> Result<Response, StatusCode> {
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some(ORIGIN) {
        return Err(StatusCode::FORBIDDEN);
    }

    let return_to = analytics_application::return_target::checked(&form.next)
        .unwrap_or("/dashboard")
        .to_owned();
    let username = form.username.trim().to_owned();
    if !analytics_domain::login_input::valid(&form.username, "admin") {
        return Ok(login_feedback(
            StatusCode::UNAUTHORIZED,
            "",
            "0",
            &return_to,
        ));
    }

    let mut attempt =
        analytics_persistence::login_attempt::LoginAttempt::begin(&state.pool, &username)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let Some(mut attempt) = attempt.take() else {
        return Ok(login_feedback(
            StatusCode::UNAUTHORIZED,
            &username,
            "0",
            &return_to,
        ));
    };
    let now = attempt
        .now()
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let wait = attempts::pending(&mut attempt, now)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if wait != "0" {
        attempt
            .commit()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(login_feedback(
            StatusCode::TOO_MANY_REQUESTS,
            &username,
            &wait,
            &return_to,
        ));
    }

    let encoded = attempt.password_hash.clone();
    let fingerprint = digest(&encoded);
    let restricted = attempt.initial;
    let shape = analytics_domain::login_input::valid(&form.username, &form.password);
    let verified = if shape {
        tokio::task::spawn_blocking(move || {
            let parsed = PasswordHash::new(&encoded).map_err(|_| ())?;
            Argon2::default()
                .verify_password(form.password.as_bytes(), &parsed)
                .map_err(|_| ())
        })
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
        .is_ok()
    } else {
        false
    };
    if !verified {
        let now = attempt
            .now()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        let wait = attempts::reject(&mut attempt, now)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        attempt
            .commit()
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Ok(login_feedback(
            StatusCode::UNAUTHORIZED,
            &username,
            &wait,
            &return_to,
        ));
    }
    attempts::accepted(&mut attempt)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let raw = format!(
        "{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    );
    let seconds = attempt
        .session(&digest(&raw), &fingerprint)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    Ok(renewed(
        Redirect::to(if restricted {
            "/change-password"
        } else {
            &return_to
        }),
        &raw,
        seconds,
    ))
}
