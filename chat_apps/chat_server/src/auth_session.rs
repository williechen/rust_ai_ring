use axum::http::{HeaderMap, StatusCode, header};
use shared::auth::{cookie_token, token_digest};
use sqlx::SqlitePool;

fn session_cookie(headers: &HeaderMap) -> Result<String, StatusCode> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| cookie_token(value, "chat_session"))
        .ok_or(StatusCode::UNAUTHORIZED)
}

/// Chat SQLite adapter: caller supplies this application's independent DB pool.
/// No plaintext bearer token is ever stored in SQLite.
pub async fn active_member(pool: &SqlitePool, headers: &HeaderMap) -> Result<String, StatusCode> {
    let raw = session_cookie(headers)?;
    let member: Option<String> = sqlx::query_scalar(
        r#"SELECT u.id 
        FROM mbr_session s
        JOIN mbr_user u 
            ON u.id=s.mbr_user_id
        WHERE s.token_hash=?1 
          AND s.expires_at>unixepoch()
          AND u.user_status='active'"#,
    )
    .bind(token_digest(&raw))
    .fetch_optional(pool)
    .await
    .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    member.ok_or(StatusCode::UNAUTHORIZED)
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
        headers.insert(header::COOKIE, format!("x=1; chat_session={raw}").parse().unwrap());
        assert_eq!(session_cookie(&headers).unwrap(), raw);
        assert_eq!(csrf_for_logout(&headers).unwrap(), token_digest(&format!("logout:{raw}")));
        assert_eq!(csrf_for_create_room(&headers).unwrap(), token_digest(&format!("create-room:{raw}")));
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
