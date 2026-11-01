use analytics_application::viewer_access::{
    AccessError, Actor, SourceChanges, SourceChoice, ViewerAccessStore,
};
use sha2::{Digest, Sha256};

pub struct PgViewerAccess(pub sqlx::PgPool);

impl ViewerAccessStore for PgViewerAccess {
    async fn update(
        &self,
        actor: &Actor,
        target: &str,
        changes: SourceChanges,
    ) -> Result<(), AccessError> {
        let mut tx = self.0.begin().await.map_err(|_| AccessError::Unavailable)?;
        // 鎖定操作者目前憑證與Session；不信任先前頁面的權限快照。
        let auth:Option<(String,String)>=sqlx::query_as("SELECT u.user_pd,s.credential_fingerprint FROM uac_user u JOIN uac_login_session s ON s.user_id=u.id WHERE u.id=$1 AND s.token_hash=$2 AND s.expires_at>floor(extract(epoch from now()))::bigint AND s.restricted=FALSE AND u.user_status='active' AND u.is_init_pd=FALSE FOR SHARE OF u,s")
            .bind(&actor.id).bind(&actor.token_hash).fetch_optional(&mut *tx).await.map_err(|_|AccessError::Unavailable)?;
        let (password, fingerprint) = auth.ok_or(AccessError::Denied)?;
        if Sha256::digest(password.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
            != fingerprint
        {
            return Err(AccessError::Denied);
        }
        let allowed: bool = sqlx::query_scalar(crate::page_permissions::PERMISSION_SQL)
            .bind(&actor.token_hash)
            .bind("/accounts/new")
            .bind("update")
            .fetch_one(&mut *tx)
            .await
            .map_err(|_| AccessError::Unavailable)?;
        if !allowed {
            return Err(AccessError::Denied);
        }
        let found:Option<String>=sqlx::query_scalar("SELECT u.id FROM uac_user u WHERE u.id=$1 AND u.user_status='active' AND EXISTS(SELECT 1 FROM uac_user_role r WHERE r.user_id=u.id AND r.role_id='analytics-viewer') AND NOT EXISTS(SELECT 1 FROM uac_user_role r WHERE r.user_id=u.id AND r.role_id='analytics-admin') FOR UPDATE OF u")
            .bind(target).fetch_optional(&mut *tx).await.map_err(|_|AccessError::Unavailable)?;
        if found.is_none() {
            return Err(AccessError::NotFound);
        }
        let nodes:i64=sqlx::query_scalar("SELECT count(*) FROM uac_acl WHERE id IN('analytics-chat','analytics-pet') AND acl_category='page' AND acl_status='active'").fetch_one(&mut *tx).await.map_err(|_|AccessError::Unavailable)?;
        if nodes != 2 {
            return Err(AccessError::Unavailable);
        }
        for (id, choice) in [
            ("analytics-chat", changes.chat),
            ("analytics-pet", changes.pet),
        ] {
            if choice == SourceChoice::Inherit {
                sqlx::query("DELETE FROM uac_user_acl WHERE user_id=$1 AND acl_id=$2")
                    .bind(target)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AccessError::Unavailable)?;
            } else {
                sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,$2,$3,now()) ON CONFLICT(user_id,acl_id) DO UPDATE SET acl_filter=EXCLUDED.acl_filter")
                    .bind(target).bind(id).bind(choice.key()).execute(&mut *tx).await.map_err(|_|AccessError::Unavailable)?;
            }
        }
        let result = format!("chat={};pet={}", changes.chat.key(), changes.pet.key());
        sqlx::query("INSERT INTO analytics_account_audit(id,actor_id,target_id,action,result) VALUES($1,$2,$3,'update_viewer_sources',$4)").bind(uuid::Uuid::new_v4().to_string()).bind(&actor.id).bind(target).bind(result).execute(&mut *tx).await.map_err(|_|AccessError::Unavailable)?;
        tx.commit().await.map_err(|_| AccessError::Unavailable)
    }
}
