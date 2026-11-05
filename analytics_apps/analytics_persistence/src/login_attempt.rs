use analytics_application::login_attempt::AttemptStore;
use shared::login_wait::WaitState;
use sqlx::{PgPool, Postgres, Transaction};

pub struct LoginAttempt {
    tx: Transaction<'static, Postgres>,
    pub user_id: String,
    pub password_hash: String,
    pub initial: bool,
}

impl LoginAttempt {
    pub async fn begin(pool: &PgPool, name: &str) -> Result<Option<Self>, String> {
        let mut tx = pool.begin().await.map_err(|_| "無法開始登入交易")?;
        // 帳號列鎖將同帳號的跨連線、跨程序登入序列化。
        let rows:Vec<(String,String,bool,String)>=sqlx::query_as("SELECT id,user_pd,is_init_pd,user_status FROM uac_user WHERE lower(user_acnt)=lower($1) ORDER BY id LIMIT 2 FOR UPDATE").bind(name).fetch_all(&mut *tx).await.map_err(|_|"無法讀取帳號")?;
        if rows.len() != 1 || rows[0].3 != "active" {
            return Ok(None);
        }
        let (user_id, password_hash, initial, _) = rows.into_iter().next().unwrap();
        sqlx::query("INSERT INTO uac_login_attempt(user_id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(&user_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| "無法準備等待資料")?;
        Ok(Some(Self {
            tx,
            user_id,
            password_hash,
            initial,
        }))
    }
    pub async fn now(&mut self) -> Result<u64, String> {
        let n: i64 =
            sqlx::query_scalar("SELECT floor(extract(epoch from clock_timestamp()))::bigint")
                .fetch_one(&mut *self.tx)
                .await
                .map_err(|_| "無法讀取時鐘")?;
        u64::try_from(n).map_err(|_| "時鐘超出範圍".into())
    }
    pub async fn commit(self) -> Result<(), String> {
        self.tx
            .commit()
            .await
            .map_err(|_| "無法提交等待資料".into())
    }
    pub async fn session(mut self, token_hash: &str, fingerprint: &str) -> Result<(), String> {
        sqlx::query("DELETE FROM uac_login_session WHERE expires_at<=floor(extract(epoch from clock_timestamp()))::bigint").execute(&mut *self.tx).await.map_err(|_|"無法清理Session")?;
        sqlx::query("INSERT INTO uac_login_session(token_hash,user_id,credential_fingerprint,restricted,expires_at) VALUES($1,$2,$3,$4,floor(extract(epoch from clock_timestamp()))::bigint+1800)").bind(token_hash).bind(&self.user_id).bind(fingerprint).bind(self.initial).execute(&mut *self.tx).await.map_err(|_|"無法建立Session")?;
        self.commit().await
    }
}

impl AttemptStore for LoginAttempt {
    async fn read(&mut self) -> Result<WaitState, String> {
        let r: (String, String, String) = sqlx::query_as(
            "SELECT failures,next_delay,blocked_until FROM uac_login_attempt WHERE user_id=$1",
        )
        .bind(&self.user_id)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(|_| "無法讀取等待")?;
        Ok(WaitState {
            failures: r.0,
            delay: r.1,
            until: r.2,
        })
    }
    async fn write(&mut self, s: &WaitState) -> Result<(), String> {
        sqlx::query("UPDATE uac_login_attempt SET failures=$1,next_delay=$2,blocked_until=$3 WHERE user_id=$4").bind(&s.failures).bind(&s.delay).bind(&s.until).bind(&self.user_id).execute(&mut *self.tx).await.map_err(|_|"無法保存等待")?;
        Ok(())
    }
}
