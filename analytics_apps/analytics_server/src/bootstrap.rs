use argon2::{Argon2, PasswordHasher};

pub async fn initialize(pool: &sqlx::PgPool, password: String) -> Result<String, String> {
    if !(8..=128).contains(&password.len()) {
        return Err("密碼須為 8 到 128 bytes".into());
    }
    let hash = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes())
            .map(|hash| hash.to_string())
            .map_err(|_| "密碼雜湊失敗")
    })
    .await
    .map_err(|_| "密碼工作失敗")??;

    let mut tx = pool.begin().await.map_err(|_| "無法開始初始化")?;
    // 同時啟動的初始化會序列化；不使用先查後寫而無鎖的競態。
    sqlx::query("LOCK TABLE uac_user IN EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await
        .map_err(|_| "無法鎖定帳號表")?;

    let exists: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
            SELECT 1 
            FROM uac_user 
            WHERE lower(user_acnt)='admin')"#,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "無法查核管理員")?;
    if exists {
        return Err("管理員已存在；不覆寫密碼".into());
    }

    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(r#"INSERT INTO uac_user (id
        ,user_acnt
        ,user_pd
        ,user_email
        ,user_name
        ,user_status
        ,is_init_pd
        ,created_at
        ,updated_at
    ) VALUES ($1,'admin',$2,'admin@localhost.invalid','管理員','active',TRUE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"#)
        .bind(&id)
        .bind(hash)
        .execute(&mut *tx).await.map_err(|_| "管理員寫入失敗")?;

    tx.commit().await.map_err(|_| "管理員提交失敗")?;

    Ok(id)
}

/// 只在空帳號庫建立預設管理員；存在資料時不改任何帳密或狀態。
pub async fn ensure_default(pool: &sqlx::PgPool) -> Result<String, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "無法開始初始化".to_owned())?;
    sqlx::query("LOCK TABLE uac_user IN EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await
        .map_err(|_| "無法鎖定帳號表".to_owned())?;
    let admins: Vec<(String,)> =
        sqlx::query_as("SELECT id FROM uac_user WHERE lower(user_acnt)='admin' LIMIT 2")
            .fetch_all(&mut *tx)
            .await
            .map_err(|_| "無法查核管理員".to_owned())?;
    if admins.len() > 1 {
        return Err("管理員身分重複；停止初始化，需人工查核".into());
    }
    if let Some((id,)) = admins.into_iter().next() {
        tx.commit()
            .await
            .map_err(|_| "初始化查核提交失敗".to_owned())?;
        return Ok(id);
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM uac_user")
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| "無法查核帳號庫".to_owned())?;
    if count != 0 {
        return Err("非空帳號庫缺管理員；不自動重建預設帳密".into());
    }
    // 沿用 Argon2 0.6 PasswordHasher 的隨機 salt，不儲存明碼。
    let hash = tokio::task::spawn_blocking(|| {
        Argon2::default()
            .hash_password(b"admin")
            .map(|h| h.to_string())
            .map_err(|_| "密碼雜湊失敗".to_owned())
    })
    .await
    .map_err(|_| "密碼工作失敗".to_owned())??;
    let id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO uac_user (id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at)
         VALUES ($1,'admin',$2,'admin@localhost.invalid','管理員','active',TRUE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)"
    ).bind(&id).bind(hash).execute(&mut *tx).await
        .map_err(|_| "管理員寫入失敗".to_owned())?;
    tx.commit().await.map_err(|_| "管理員提交失敗".to_owned())?;
    Ok(id)
}

#[cfg(all(test, feature = "postgres-integration"))]
mod day22_tests {
    use super::*;
    use argon2::{PasswordHash, PasswordVerifier};
    #[sqlx::test(migrations = "../migrations")]
    async fn day22_bootstrap_serializes_and_preserves_hash(pool: sqlx::PgPool) {
        let password = "lesson-only-password".to_owned();
        let (a, b) = tokio::join!(
            initialize(&pool, password.clone()),
            initialize(&pool, password.clone())
        );
        assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
        assert!(initialize(&pool, "short".into()).await.is_err());
        let (count,): (i64,) = sqlx::query_as("SELECT count(*) FROM uac_user")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
        let (hash, must_change): (String, bool) =
            sqlx::query_as("SELECT user_pd,is_init_pd FROM uac_user WHERE user_acnt='admin'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(must_change);
        assert!(hash.starts_with("$argon2id$"));
        Argon2::default()
            .verify_password(password.as_bytes(), &PasswordHash::new(&hash).unwrap())
            .unwrap();
        assert!(
            initialize(&pool, "different-lesson-password".into())
                .await
                .is_err()
        );
        let (after,): (String,) =
            sqlx::query_as("SELECT user_pd FROM uac_user WHERE user_acnt='admin'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after, hash);
    }
}

#[cfg(all(test, feature = "postgres-integration"))]
mod day34_default_tests {
    use super::ensure_default;
    use argon2::{Argon2, PasswordHash, PasswordVerifier};
    use axum::{
        body::{Body, to_bytes},
        http::{Request, StatusCode},
    };
    use sha2::{Digest, Sha256};
    use std::sync::Arc;
    use tower::ServiceExt;

    #[sqlx::test(migrations = "../migrations")]
    async fn day34_parallel_start_preserves_one_salted_admin(pool: sqlx::PgPool) {
        let (a, b) = tokio::join!(ensure_default(&pool), ensure_default(&pool));
        assert_eq!(a.unwrap(), b.unwrap());
        let rows: Vec<(String, bool)> = sqlx::query_as(
            "SELECT user_pd,is_init_pd FROM uac_user WHERE lower(user_acnt)='admin'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert!(rows[0].1);
        let hash = rows[0].0.clone();
        assert_ne!(hash, "admin");
        Argon2::default()
            .verify_password(b"admin", &PasswordHash::new(&hash).unwrap())
            .unwrap();
        ensure_default(&pool).await.unwrap();
        let after: String =
            sqlx::query_scalar("SELECT user_pd FROM uac_user WHERE user_acnt='admin'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after, hash);
    }

    #[sqlx::test(migrations = "../migrations")]
    async fn day34_real_login_change_and_restart_never_reset_password(pool: sqlx::PgPool) {
        ensure_default(&pool).await.unwrap();
        let app = crate::password_login::build(crate::IngestState {
            pool: pool.clone(),
            tokens: Arc::new(crate::IngestTokens {
                chat: "fixture-chat".into(),
                pet: "fixture-pet".into(),
            }),
        });
        let login_page = app
            .clone()
            .oneshot(Request::get("/login").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(login_page.status(), StatusCode::OK);
        let body = to_bytes(login_page.into_body(), 16384).await.unwrap();
        assert!(
            std::str::from_utf8(&body)
                .unwrap()
                .contains("analytics-password")
        );

        let login = app
            .clone()
            .oneshot(
                Request::post("/login")
                    .header("origin", "http://127.0.0.1:3100")
                    .header("content-type", "application/x-www-form-urlencoded")
                    .body(Body::from("username=admin&password=admin"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(login.status(), StatusCode::SEE_OTHER);
        assert_eq!(login.headers()["location"], "/change-password");
        let cookie = login.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let raw = cookie.strip_prefix("analytics_session=").unwrap();
        let csrf: String = Sha256::digest(format!("change-password:{raw}").as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();

        let denied = app
            .clone()
            .oneshot(
                Request::get("/dashboard")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::SEE_OTHER);
        assert_eq!(denied.headers()["location"], "/change-password");
        let change_page = app
            .clone()
            .oneshot(
                Request::get("/change-password")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(change_page.status(), StatusCode::OK);
        let body = to_bytes(change_page.into_body(), 16384).await.unwrap();
        assert!(
            std::str::from_utf8(&body)
                .unwrap()
                .contains("change-current")
        );

        let changed = app.clone().oneshot(Request::post("/change-password")
            .header("origin", "http://127.0.0.1:3100")
            .header("cookie", &cookie)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(format!("current_password=admin&new_password=changed-pass-34&confirm_password=changed-pass-34&email=admin%40example.test&csrf={csrf}"))).unwrap()
        ).await.unwrap();
        assert_eq!(changed.status(), StatusCode::SEE_OTHER);
        assert_eq!(changed.headers()["location"], "/login");
        let before: (String, String, bool) = sqlx::query_as(
            "SELECT user_pd,user_email,is_init_pd FROM uac_user WHERE user_acnt='admin'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(before.1, "admin@example.test");
        assert!(!before.2);
        ensure_default(&pool).await.unwrap(); // 與重啟啟動路徑相同。
        let after: (String, String, bool) = sqlx::query_as(
            "SELECT user_pd,user_email,is_init_pd FROM uac_user WHERE user_acnt='admin'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after, before);
        let stale = app
            .clone()
            .oneshot(
                Request::get("/api/v1/events/web_chat/summary")
                    .header("cookie", &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stale.status(), StatusCode::UNAUTHORIZED);
        for (password, expected) in [
            ("admin", StatusCode::UNAUTHORIZED),
            ("changed-pass-34", StatusCode::SEE_OTHER),
        ] {
            let result = app
                .clone()
                .oneshot(
                    Request::post("/login")
                        .header("origin", "http://127.0.0.1:3100")
                        .header("content-type", "application/x-www-form-urlencoded")
                        .body(Body::from(format!("username=admin&password={password}")))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(result.status(), expected);
            if expected == StatusCode::SEE_OTHER {
                assert_eq!(result.headers()["location"], "/dashboard");
            }
        }
    }
}
