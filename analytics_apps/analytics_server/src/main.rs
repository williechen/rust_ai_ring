use analytics_server::{IngestState, IngestTokens, password_login};
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    // 啟動時 fail-closed；不得把密鑰值寫入日誌。
    let database_url = std::env::var("ANALYTICS_DATABASE_URL")?;

    let pool = sqlx::PgPool::connect(&database_url).await?;
    sqlx::migrate!("../migrations").run(&pool).await?;

    let chat = std::env::var("ANALYTICS_CHAT_TOKEN")?;
    let pet = std::env::var("ANALYTICS_PET_TOKEN")?;

    if chat.is_empty() || pet.is_empty() || chat == pet || chat.len() > 512 || pet.len() > 512 {
        return Err("來源憑證設定不合法".into());
    }

    analytics_server::bootstrap::ensure_default(&pool)
        .await
        .map_err(std::io::Error::other)?;

    let app = password_login::build(IngestState {
        pool,
        tokens: Arc::new(IngestTokens { chat, pet }),
    });

    println!("Stats server running on http://127.0.0.1:3100");

    // 本地教學切片；首次改密碼已接通，完整管理與角色授權另行完成
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3100").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
