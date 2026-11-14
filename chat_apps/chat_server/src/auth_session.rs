use axum::http::{HeaderMap, StatusCode, header};
use shared::auth::{cookie_token, token_digest};
use sqlx::SqlitePool;

pub(crate) fn member_cookie(raw: &str) -> String {
    let seconds = chat_application::MEMBER_SESSION_IDLE_SECONDS;
    format!("chat_session={raw}; HttpOnly; SameSite=Lax; Path=/; Max-Age={seconds}")
}

/// 只接受唯一的 Chat Cookie；其他名稱可以共存，缺少與畸形分開處理。
pub(crate) fn optional_session_cookie(headers: &HeaderMap) -> Result<Option<String>, StatusCode> {
    let mut found = None;
    for value in headers.get_all(header::COOKIE).iter() {
        let value = value.to_str().map_err(|_| StatusCode::UNAUTHORIZED)?;
        for part in value.split(';').map(str::trim) {
            let Some((key, _)) = part.split_once('=') else {
                if part == "chat_session" {
                    return Err(StatusCode::UNAUTHORIZED);
                }
                continue;
            };
            if key.trim() != "chat_session" {
                continue;
            }
            if key != "chat_session" || found.is_some() {
                return Err(StatusCode::UNAUTHORIZED);
            }
            found = Some(cookie_token(part, "chat_session").ok_or(StatusCode::UNAUTHORIZED)?);
        }
    }
    Ok(found)
}

pub(crate) fn session_cookie(headers: &HeaderMap) -> Result<String, StatusCode> {
    optional_session_cookie(headers)?.ok_or(StatusCode::UNAUTHORIZED)
}

/// Chat SQLite adapter: caller supplies this application's independent DB pool.
/// No plaintext bearer token is ever stored in SQLite.
pub async fn active_member(pool: &SqlitePool, headers: &HeaderMap) -> Result<String, StatusCode> {
    let raw = session_cookie(headers)?;
    let member = chat_persistence::member_session::active_member_id(pool, &token_digest(&raw))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    member.ok_or(StatusCode::UNAUTHORIZED)
}

pub(crate) async fn renew_member_cookie(
    pool: &SqlitePool,
    headers: &HeaderMap,
) -> Result<String, StatusCode> {
    let raw = session_cookie(headers)?;
    let renewed = chat_persistence::member_session::renew(pool, &token_digest(&raw))
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    if !renewed {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(member_cookie(&raw))
}

pub fn csrf_for_logout(headers: &HeaderMap) -> Result<String, StatusCode> {
    let raw = session_cookie(headers)?;
    Ok(token_digest(&format!("logout:{raw}")))
}

pub fn csrf_for_create_room(headers: &HeaderMap) -> Result<String, StatusCode> {
    let raw = session_cookie(headers)?;
    Ok(token_digest(&format!("create-room:{raw}")))
}

// Add to apps/server/src/auth_session.rs at the bottom:
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cookie_parser_and_both_csrf_callers_keep_the_same_contract() {
        let mut headers = HeaderMap::new();
        assert_eq!(session_cookie(&headers), Err(StatusCode::UNAUTHORIZED));
        headers.insert(header::COOKIE, "chat_session=bad".parse().unwrap());
        assert_eq!(session_cookie(&headers), Err(StatusCode::UNAUTHORIZED));
        let raw = "b".repeat(64);
        headers.insert(
            header::COOKIE,
            format!("x=1; chat_session={raw}").parse().unwrap(),
        );
        assert_eq!(session_cookie(&headers).unwrap(), raw);
        assert_eq!(
            csrf_for_logout(&headers).unwrap(),
            token_digest(&format!("logout:{raw}"))
        );
        assert_eq!(
            csrf_for_create_room(&headers).unwrap(),
            token_digest(&format!("create-room:{raw}"))
        );
    }

    #[tokio::test]
    async fn session_adapter_obeys_revocation_and_account_state() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261018073931_create_auth_user.sql"
        ))
        .execute(&db)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261020152737_mbr_sessions.sql"
        ))
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO mbr_user
            (id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at)
            VALUES('member-27','learner','hash','lesson@example.invalid','Learner','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
            .execute(&db).await.unwrap();
        let raw = "a".repeat(64);
        sqlx::query(
            "INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at)
            VALUES('session-27','member-27',?1,unixepoch()+60)",
        )
        .bind(token_digest(&raw))
        .execute(&db)
        .await
        .unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            format!("other=0; chat_session={raw}").parse().unwrap(),
        );
        assert_eq!(active_member(&db, &headers).await.unwrap(), "member-27");
        assert_eq!(
            csrf_for_logout(&headers).unwrap(),
            token_digest(&format!("logout:{raw}"))
        );
        sqlx::query("UPDATE mbr_user SET user_status='disabled' WHERE id='member-27'")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(
            active_member(&db, &headers).await.unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
        sqlx::query("UPDATE mbr_user SET user_status='active' WHERE id='member-27'")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM mbr_session WHERE id='session-27'")
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(
            active_member(&db, &headers).await.unwrap_err(),
            StatusCode::UNAUTHORIZED
        );
    }
}
