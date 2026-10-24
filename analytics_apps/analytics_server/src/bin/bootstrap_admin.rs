#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let url = std::env::var("ANALYTICS_DATABASE_URL")?;
    let pool = sqlx::PgPool::connect(&url).await?;

    sqlx::migrate!("../migrations").run(&pool).await?;

    analytics_server::bootstrap::ensure_default(&pool)
        .await
        .map_err(std::io::Error::other)?;

    println!("管理員初始化查核完成；既有帳密與狀態保持原值");
    Ok(())
}
