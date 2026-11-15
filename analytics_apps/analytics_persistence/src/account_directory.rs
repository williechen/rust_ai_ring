use sqlx::PgPool;

pub async fn session_actor(pool: &PgPool, token_hash: &str) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        r#"SELECT user_id 
        FROM uac_login_session 
        WHERE token_hash=$1"#,
    )
    .bind(token_hash)
    .fetch_optional(pool)
    .await
}

/// 僅投影可管理的 active viewer；排除同時具管理者角色的帳號。
pub async fn active_viewers(
    pool: &PgPool,
) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
    sqlx::query_as(
        r#"SELECT u.id
            ,u.user_acnt
            ,COALESCE((
                SELECT acl_filter 
                FROM uac_user_acl a 
                WHERE a.user_id=u.id 
                    AND a.acl_id='analytics-chat'),'inherit')
            ,COALESCE((
                SELECT acl_filter 
                FROM uac_user_acl a 
                WHERE a.user_id=u.id 
                    AND a.acl_id='analytics-pet'),'inherit') 
            FROM uac_user u 
            JOIN uac_user_role ur 
                ON ur.user_id=u.id 
            WHERE ur.role_id='analytics-viewer' 
                AND u.user_status='active' 
                AND NOT EXISTS(
                    SELECT 1 
                    FROM uac_user_role ar 
                    WHERE ar.user_id=u.id 
                        AND ar.role_id='analytics-admin') 
            ORDER BY u.user_acnt 
            LIMIT 100"#,
    )
    .fetch_all(pool)
    .await
}
