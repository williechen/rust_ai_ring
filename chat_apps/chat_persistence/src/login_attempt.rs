use chat_application::login_attempt::AttemptStore;
use shared::login_wait::WaitState;
use sqlx::{Sqlite, SqlitePool, Transaction};

pub struct LoginAttempt {
    tx: Transaction<'static, Sqlite>,
    pub user_id: String,
    pub password_hash: String,
    pub initial: bool,
}

impl LoginAttempt {
    /// SQLite立即取得寫入交易，避免多個程序同時驗密碼後重複累加。
    pub async fn begin(pool: &SqlitePool, name: &str) -> Result<Option<Self>, String> {
        let mut tx = pool
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|_| "無法開始登入交易")?;
        let row:Option<(String,String,bool,bool)>=sqlx::query_as("SELECT id,user_pd,user_status='active',is_init_pd FROM mbr_user WHERE lower(user_acnt)=lower(?1)").bind(name).fetch_optional(&mut *tx).await.map_err(|_|"無法讀取登入帳號")?;
        let Some((user_id, password_hash, true, initial)) = row else {
            tx.commit().await.map_err(|_| "無法結束登入交易")?;
            return Ok(None);
        };
        sqlx::query("INSERT OR IGNORE INTO mbr_login_attempt(user_id) VALUES(?1)")
            .bind(&user_id)
            .execute(&mut *tx)
            .await
            .map_err(|_| "無法準備登入等待資料")?;
        Ok(Some(Self {
            tx,
            user_id,
            password_hash,
            initial,
        }))
    }

    pub async fn now(&mut self) -> Result<u64, String> {
        let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
            .fetch_one(&mut *self.tx)
            .await
            .map_err(|_| "無法讀取登入時鐘")?;
        u64::try_from(now).map_err(|_| "登入時鐘超出範圍".into())
    }

    pub async fn commit(self) -> Result<(), String> {
        self.tx
            .commit()
            .await
            .map_err(|_| "無法提交登入等待資料".into())
    }

    pub async fn session(mut self, id: &str, digest: &str) -> Result<(), String> {
        sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES(?1,?2,?3,unixepoch()+86400)").bind(id).bind(&self.user_id).bind(digest).execute(&mut *self.tx).await.map_err(|_|"無法建立登入工作階段")?;
        self.commit().await
    }
}

impl AttemptStore for LoginAttempt {
    async fn read(&mut self) -> Result<WaitState, String> {
        let row: (String, String, String) = sqlx::query_as(
            "SELECT failures,next_delay,blocked_until FROM mbr_login_attempt WHERE user_id=?1",
        )
        .bind(&self.user_id)
        .fetch_one(&mut *self.tx)
        .await
        .map_err(|_| "無法讀取登入等待資料")?;
        Ok(WaitState {
            failures: row.0,
            delay: row.1,
            until: row.2,
        })
    }

    async fn write(&mut self, s: &WaitState) -> Result<(), String> {
        sqlx::query("UPDATE mbr_login_attempt SET failures=?1,next_delay=?2,blocked_until=?3 WHERE user_id=?4").bind(&s.failures).bind(&s.delay).bind(&s.until).bind(&self.user_id).execute(&mut *self.tx).await.map_err(|_|"無法儲存登入等待資料")?;
        Ok(())
    }
}
