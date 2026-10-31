use crate::{
    IngestState,
    password_login::{digest, renewed, session},
};
use argon2::{Argon2, PasswordHasher};
use axum::{
    Form,
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, Redirect, Response},
};
use leptos::prelude::*;

async fn account_manager(
    state: &IngestState,
    headers: &HeaderMap,
) -> Result<(String, String), StatusCode> {
    let (restricted, raw) = session(state, headers).await?;
    if restricted {
        return Err(StatusCode::FORBIDDEN);
    }
    // Page/action authority is ACL-based, including independent user additions.
    // The production page_acl middleware already checks the same capability.
    let actor: Option<String> =
        sqlx::query_scalar("SELECT user_id FROM uac_login_session WHERE token_hash=$1")
            .bind(digest(&raw))
            .fetch_optional(&state.pool)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok((actor.ok_or(StatusCode::UNAUTHORIZED)?, raw))
}

#[derive(Default, serde::Deserialize)]
pub(crate) struct Notice {
    created: Option<String>,
}

pub(crate) async fn page(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Query(query): Query<Notice>,
) -> Result<Response, StatusCode> {
    let (_, raw) = account_manager(&state, &headers).await?;
    let csrf = digest(&format!("create-account:{raw}"));
    let message = if query.created.as_deref() == Some("1") {
        "已建立唯讀帳號；請安全交付初始密碼，首次登入須改密碼。"
    } else {
        ""
    };
    let mut html=view!{
        <html lang="zh-Hant"><head><title>"新增唯讀帳號"</title></head><body><main>
        <h1>"新增唯讀帳號"</h1><p role="status">{message}</p>
        <form method="post" action="/accounts">
        <input type="hidden" name="csrf" value=csrf/>
        <label>"帳號"<input name="username" required minlength="3" maxlength="64" autocomplete="off"/></label>
        <label>"初始密碼"<input type="password" name="password" required minlength="8" maxlength="128" autocomplete="new-password"/></label>
        <label>"Email"<input type="email" name="email" required maxlength="254"/></label>
        <label><input type="checkbox" name="chat"/>"可讀 Chat"</label>
        <label><input type="checkbox" name="pet"/>"可讀 Pet"</label>
        <button type="submit">"建立唯讀帳號"</button></form>
        <a href="/dashboard">"返回後台"</a>
        </main></body></html>
    }.to_html();
    let can_create =
        crate::page_acl::allowed(&state, &raw, "/accounts/new", crate::page_acl::Crud::Create)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !can_create {
        if let Some(start) = html.find("<form method=\"post\" action=\"/accounts\"") {
            if let Some(end) = html[start..].find("</form>") {
                html.replace_range(start..start + end + 7, "<p>沒有新增 action。</p>");
            }
        }
    }
    Ok(renewed(
        Html(crate::page_acl::decorate(&state, &raw, html).await?),
        &raw,
    ))
}

#[derive(serde::Deserialize)]
pub(crate) struct NewViewer {
    username: String,
    password: String,
    email: String,
    chat: Option<String>,
    pet: Option<String>,
    csrf: String,
}

fn db_error(e: sqlx::Error) -> StatusCode {
    if let sqlx::Error::Database(d) = &e {
        if d.is_unique_violation() {
            return StatusCode::CONFLICT;
        }
    }
    StatusCode::SERVICE_UNAVAILABLE
}

pub(crate) async fn create(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Form(form): Form<NewViewer>,
) -> Result<Response, StatusCode> {
    let (actor, raw) = account_manager(&state, &headers).await?;
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some("http://127.0.0.1:3100")
        || form.csrf != digest(&format!("create-account:{raw}"))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let result = create_inner(&state, &actor, &raw, form).await;
    if let Err(status) = &result {
        sqlx::query("INSERT INTO analytics_account_audit(id,actor_id,target_id,action,result) VALUES($1,$2,'uncreated','create_viewer_rejected',$3)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&actor).bind(status.as_str())
            .execute(&state.pool).await.map_err(db_error)?;
    }
    result
}

async fn create_inner(
    state: &IngestState,
    actor: &str,
    raw: &str,
    form: NewViewer,
) -> Result<Response, StatusCode> {
    let username = form.username.trim().to_ascii_lowercase();
    let email = form.email.trim().to_ascii_lowercase();
    let parts: Vec<_> = email.split('@').collect();
    if !(3..=64).contains(&username.len())
        || !username
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        || !(8..=128).contains(&form.password.len())
        || email.len() > 254
        || !email.is_ascii()
        || email
            .bytes()
            .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        || parts.len() != 2
        || parts[0].is_empty()
        || !parts[1].contains('.')
        || parts[1].starts_with('.')
        || parts[1].ends_with('.')
        || (form.chat.is_none() && form.pet.is_none())
        || form.chat.as_deref().is_some_and(|v| v != "on")
        || form.pet.as_deref().is_some_and(|v| v != "on")
    {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let encoded = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(form.password.as_bytes())
            .map(|h| h.to_string())
    })
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut tx = state.pool.begin().await.map_err(db_error)?;
    // Lock identity/credential state, then recheck page + Create action in this transaction.
    let authorized:Option<(String,String)>=sqlx::query_as(
        "SELECT u.user_pd,s.credential_fingerprint FROM uac_user u JOIN uac_login_session s ON s.user_id=u.id
         WHERE u.id=$1 AND s.token_hash=$2 AND s.expires_at>floor(extract(epoch from now()))::bigint
          AND s.restricted=FALSE AND u.user_status='active' AND u.is_init_pd=FALSE FOR SHARE OF u,s")
        .bind(actor).bind(digest(raw)).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let (credential, fingerprint) = authorized.ok_or(StatusCode::FORBIDDEN)?;
    if digest(&credential) != fingerprint {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let permitted: bool = sqlx::query_scalar(crate::page_acl::PERMISSION_SQL)
        .bind(digest(raw))
        .bind("/accounts/new")
        .bind("create")
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
    if !permitted {
        return Err(StatusCode::FORBIDDEN);
    }
    sqlx::query("INSERT INTO uac_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES($1,$2,$3,$4,$2,'active',TRUE,now(),now())")
        .bind(&id).bind(username).bind(encoded).bind(email).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query(
        "INSERT INTO uac_user_role(user_id,role_id,created_at) VALUES($1,'analytics-viewer',now())",
    )
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    // Viewer role intentionally lacks password-update. Provision this required self action
    // as an explicit personal grant, not by restoring the removed role grant.
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-password-update','allow',now())")
        .bind(&id).execute(&mut *tx).await.map_err(db_error)?;
    for (enabled, acl) in [
        (form.chat.is_some(), "analytics-chat"),
        (form.pet.is_some(), "analytics-pet"),
    ] {
        if enabled {
            sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,$2,'allow',now())").bind(&id).bind(acl).execute(&mut *tx).await.map_err(db_error)?;
        }
    }
    sqlx::query("INSERT INTO analytics_account_audit(id,actor_id,target_id,action,result) VALUES($1,$2,$3,'create_viewer','created')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(actor).bind(&id).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(renewed(Redirect::to("/accounts/new?created=1"), &raw))
}
