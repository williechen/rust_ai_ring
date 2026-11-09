use crate::{
    IngestState,
    management_shell::ManagementShell,
    password_login::{digest, renewed, session},
};
use argon2::{Argon2, PasswordHasher};
use axum::{
    Form,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{Html, Redirect, Response},
};
use leptos::prelude::*;

async fn account_manager(
    state: &IngestState,
    headers: &HeaderMap,
) -> Result<(String, String, i64), StatusCode> {
    let (restricted, raw, seconds) = session(state, headers).await?;
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
    Ok((actor.ok_or(StatusCode::UNAUTHORIZED)?, raw, seconds))
}

#[derive(Default, serde::Deserialize)]
pub(crate) struct Notice {
    created: Option<String>,
    disabled: Option<String>,
    updated: Option<String>,
}

pub(crate) async fn page(
    State(state): State<IngestState>,
    headers: HeaderMap,
    Query(query): Query<Notice>,
) -> Result<Response, StatusCode> {
    let (_, raw, seconds) = account_manager(&state, &headers).await?;
    let csrf = digest(&format!("create-account:{raw}"));
    let message = if query.created.as_deref() == Some("1") {
        "已建立唯讀帳號；請安全交付初始密碼，首次登入須改密碼。"
    } else if query.disabled.as_deref() == Some("1") {
        "已停用檢視者，既有登入工作階段已撤銷。"
    } else if query.updated.as_deref() == Some("1") {
        "已儲存來源權限；既有Session下次查詢立即重新核權。"
    } else {
        ""
    };

    let can_delete =
        crate::page_acl::allowed(&state, &raw, "/accounts/new", crate::page_acl::Crud::Delete)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let can_update =
        crate::page_acl::allowed(&state, &raw, "/accounts/new", crate::page_acl::Crud::Update)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;

    let viewers: Vec<(String, String, String, String)> = if can_delete || can_update {
        sqlx::query_as("SELECT u.id,u.user_acnt,COALESCE((SELECT acl_filter FROM uac_user_acl a WHERE a.user_id=u.id AND a.acl_id='analytics-chat'),'inherit'),COALESCE((SELECT acl_filter FROM uac_user_acl a WHERE a.user_id=u.id AND a.acl_id='analytics-pet'),'inherit') FROM uac_user u JOIN uac_user_role ur ON ur.user_id=u.id WHERE ur.role_id='analytics-viewer' AND u.user_status='active' AND NOT EXISTS(SELECT 1 FROM uac_user_role ar WHERE ar.user_id=u.id AND ar.role_id='analytics-admin') ORDER BY u.user_acnt LIMIT 100")
            .fetch_all(&state.pool).await.map_err(db_error)?
    } else {
        Vec::new()
    };

    let viewer_forms=viewers.into_iter().map(|(id,username,chat,pet)| {
        let delete_action=format!("/accounts/{id}/disable");let delete_token=digest(&format!("disable-account:{raw}:{id}"));
        let update_action=format!("/accounts/{id}/sources");let update_token=digest(&format!("update-sources:{raw}:{id}"));
        let delete_form=can_delete.then(||view! { <form method="post" action=delete_action><input type="hidden" name="csrf" value=delete_token/><button type="submit">"停用帳號"</button></form> });
        let update_form=can_update.then(||view! {
            <form method="post" action=update_action>
                <input type="hidden" name="csrf" value=update_token/>
                <label>"Chat來源"<select name="chat">
                    <option value="inherit" selected=chat=="inherit">"沿用角色"</option><option value="allow" selected=chat=="allow">"個人允許"</option><option value="deny" selected=chat=="deny">"個人拒絕"</option>
                </select></label>
                <label>"Pet來源"<select name="pet">
                    <option value="inherit" selected=pet=="inherit">"沿用角色"</option><option value="allow" selected=pet=="allow">"個人允許"</option><option value="deny" selected=pet=="deny">"個人拒絕"</option>
                </select></label><button type="submit">"儲存來源權限"</button>
            </form>
        });
        view! { <li><span>{username}</span>{update_form}{delete_form}</li> }
    }).collect_view();

    let can_create =
        crate::page_acl::allowed(&state, &raw, "/accounts/new", crate::page_acl::Crud::Create)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let pages = crate::page_acl::pages(&state, &raw).await?;
    let logout_csrf = digest(&format!("logout:{raw}"));
    let html=view!{
        <html lang="zh-Hant">
            <head>
                <title>"新增唯讀帳號"</title><style>{include_str!("css/dashboard.css")}</style>
            </head>
            <body>
                <ManagementShell pages logout_csrf>
                    <h1>"新增唯讀帳號"</h1>
                    <p role="status">{message}</p>
                    {if can_create { view! { <form method="post" action="/accounts">
                        <input type="hidden" name="csrf" value=csrf/>
                        <label>"帳號"
                            <input name="username" required minlength="3" maxlength="64" autocomplete="off"/>
                        </label>
                        <label>"初始密碼"
                            <input type="password" name="password" required minlength="8" maxlength="128" autocomplete="new-password"/>
                        </label>
                        <label>"Email"
                            <input type="email" name="email" required maxlength="254"/>
                        </label>
                        <label>
                            <input type="checkbox" name="chat"/>"可讀 Chat"
                        </label>
                        <label>
                            <input type="checkbox" name="pet"/>"可讀 Pet"
                        </label>
                        <button type="submit">"建立唯讀帳號"</button>
                    </form> }.into_any() } else { view! { <p>"沒有新增 action。"</p> }.into_any() }}
                    <section>
                        <h2>"檢視者帳號"</h2>
                        <ul>{viewer_forms}</ul>
                    </section>
                </ManagementShell>
            </body>
        </html>
    }.to_html();

    Ok(renewed(Html(html), &raw, seconds))
}

#[derive(serde::Deserialize)]
pub(crate) struct DisableViewer {
    csrf: String,
}

/// 停用帳號與撤銷 Session 同一個 PG 交易提交；操作前重驗 Delete action。
pub(crate) async fn disable(
    State(state): State<IngestState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<DisableViewer>,
) -> Result<Response, StatusCode> {
    let (actor, raw, seconds) = account_manager(&state, &headers).await?;
    if actor == id
        || headers.get(header::ORIGIN).and_then(|h| h.to_str().ok())
            != Some("http://127.0.0.1:3100")
        || form.csrf != digest(&format!("disable-account:{raw}:{id}"))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let mut tx = state.pool.begin().await.map_err(db_error)?;
    let authorized: Option<(String, String)> = sqlx::query_as(
        "SELECT u.user_pd,s.credential_fingerprint FROM uac_user u
         JOIN uac_login_session s ON s.user_id=u.id
         WHERE u.id=$1 AND s.token_hash=$2 AND s.expires_at>floor(extract(epoch from now()))::bigint
           AND s.restricted=FALSE AND u.user_status='active' AND u.is_init_pd=FALSE FOR SHARE OF u,s"
    ).bind(&actor).bind(digest(&raw)).fetch_optional(&mut *tx).await.map_err(db_error)?;
    let (credential, fingerprint) = authorized.ok_or(StatusCode::FORBIDDEN)?;
    if digest(&credential) != fingerprint {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let allowed: bool = sqlx::query_scalar(crate::page_acl::PERMISSION_SQL)
        .bind(digest(&raw))
        .bind("/accounts/new")
        .bind("delete")
        .fetch_one(&mut *tx)
        .await
        .map_err(db_error)?;
    if !allowed {
        return Err(StatusCode::FORBIDDEN);
    }
    let status: Option<String> = sqlx::query_scalar(
        "SELECT u.user_status FROM uac_user u JOIN uac_user_role ur ON ur.user_id=u.id
        WHERE u.id=$1 AND ur.role_id='analytics-viewer' AND NOT EXISTS(SELECT 1 FROM uac_user_role ar WHERE ar.user_id=u.id AND ar.role_id='analytics-admin') FOR UPDATE OF u",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    match status.as_deref() {
        None => return Err(StatusCode::NOT_FOUND),
        Some("active") => (),
        _ => return Err(StatusCode::CONFLICT),
    }
    sqlx::query("UPDATE uac_user SET user_status='disabled',updated_at=now() WHERE id=$1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("DELETE FROM uac_login_session WHERE user_id=$1")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?;
    sqlx::query("INSERT INTO analytics_account_audit(id,actor_id,target_id,action,result) VALUES($1,$2,$3,'disable_viewer','disabled')")
        .bind(uuid::Uuid::new_v4().to_string()).bind(&actor).bind(&id)
        .execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)?;
    Ok(renewed(
        Redirect::to("/accounts/new?disabled=1"),
        &raw,
        seconds,
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
    let (actor, raw, seconds) = account_manager(&state, &headers).await?;
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some("http://127.0.0.1:3100")
        || form.csrf != digest(&format!("create-account:{raw}"))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let result = create_inner(&state, &actor, &raw, seconds, form).await;
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
    seconds: i64,
    form: NewViewer,
) -> Result<Response, StatusCode> {
    let username = form.username.trim().to_ascii_lowercase();
    let email = form.email.trim().to_ascii_lowercase();
    let parts: Vec<_> = email.split('@').collect();
    if !crate::account_rules::valid_viewer_username(&username)
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
    Ok(renewed(
        Redirect::to("/accounts/new?created=1"),
        &raw,
        seconds,
    ))
}

#[derive(serde::Deserialize)]
pub(crate) struct SourceForm {
    csrf: String,
    chat: String,
    pet: String,
}

pub(crate) async fn update_sources(
    State(state): State<IngestState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Form(form): Form<SourceForm>,
) -> Result<Response, StatusCode> {
    use analytics_application::viewer_access::{AccessError, Actor, SourceChanges, SourceChoice};
    let (actor, raw, seconds) = account_manager(&state, &headers).await?;
    if headers.get(header::ORIGIN).and_then(|h| h.to_str().ok()) != Some("http://127.0.0.1:3100")
        || form.csrf != digest(&format!("update-sources:{raw}:{id}"))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    let changes = SourceChanges {
        chat: SourceChoice::parse(&form.chat).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?,
        pet: SourceChoice::parse(&form.pet).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?,
    };
    let adapter = analytics_persistence::viewer_access::PgViewerAccess(state.pool.clone());
    analytics_application::viewer_access::update_sources(
        &adapter,
        &Actor {
            id: actor,
            token_hash: digest(&raw),
        },
        &id,
        changes,
    )
    .await
    .map_err(|e| match e {
        AccessError::Invalid => StatusCode::UNPROCESSABLE_ENTITY,
        AccessError::Denied => StatusCode::FORBIDDEN,
        AccessError::NotFound => StatusCode::NOT_FOUND,
        AccessError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    })?;
    Ok(renewed(
        Redirect::to("/accounts/new?updated=1"),
        &raw,
        seconds,
    ))
}
