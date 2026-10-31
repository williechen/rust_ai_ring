use sqlx::PgPool;

pub type SessionRow = (bool, bool, String, String, String);

/// The caller hashes the token; this adapter owns only the PostgreSQL lookup.
pub async fn lookup(pool: &PgPool, token_hash: &str) -> Result<Option<SessionRow>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT s.restricted
            ,u.is_init_pd
            ,u.user_pd
            ,s.credential_fingerprint
            ,u.id
        FROM uac_login_session s
        JOIN uac_user u
            ON u.id=s.user_id
        WHERE s.token_hash=$1
            AND s.expires_at>floor(extract(epoch from now()))::bigint
            AND u.user_status='active'"#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}
