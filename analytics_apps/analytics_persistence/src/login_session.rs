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

pub async fn refresh(
    pool: &PgPool,
    digest: &str,
    user_id: &str,
    password_hash: &str,
) -> Result<Option<i64>, sqlx::Error> {
    // 在同一 UPDATE statement 取得與到期時間一致的設定秒數。
    sqlx::query_scalar(
        r#"UPDATE uac_login_session AS s
           SET expires_at=floor(extract(epoch from now()))::bigint+cfg.idle_seconds
           FROM analytics_session_policy AS cfg
           WHERE cfg.id=1 AND s.token_hash=$1
             AND s.user_id=$2 AND s.credential_fingerprint=$4
             AND s.expires_at>floor(extract(epoch from now()))::bigint
             AND EXISTS (SELECT 1 FROM uac_user
                         WHERE id=s.user_id AND user_pd=$3 AND user_status='active')
           RETURNING cfg.idle_seconds"#,
    )
    .bind(digest)
    .bind(user_id)
    .bind(password_hash)
    .bind(shared::auth::token_digest(password_hash))
    .fetch_optional(pool)
    .await
}

/// 密碼變更的交易同時撤銷等待狀態與所有既有工作階段。
pub async fn revoke_after_password_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM uac_login_attempt WHERE user_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM uac_login_session WHERE user_id=$1")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub struct PgSessionRenewal<'a>(pub &'a PgPool);
impl analytics_application::session_renewal::SessionRenewal for PgSessionRenewal<'_> {
    async fn credential(
        &self,
        digest: &str,
    ) -> Result<
        Option<analytics_application::session_renewal::SessionCredential>,
        analytics_application::session_renewal::RenewalError,
    > {
        use analytics_application::session_renewal::{RenewalError, SessionCredential};
        lookup(self.0, digest)
            .await
            .map(|row| {
                row.map(
                    |(_, _, password_hash, fingerprint, user_id)| SessionCredential {
                        user_id,
                        password_hash,
                        fingerprint,
                    },
                )
            })
            .map_err(|_| RenewalError::Unavailable)
    }
    async fn refresh(
        &self,
        digest: &str,
        credential: &analytics_application::session_renewal::SessionCredential,
    ) -> Result<Option<i64>, analytics_application::session_renewal::RenewalError> {
        refresh(
            self.0,
            digest,
            &credential.user_id,
            &credential.password_hash,
        )
        .await
        .map_err(|_| analytics_application::session_renewal::RenewalError::Unavailable)
    }
}
