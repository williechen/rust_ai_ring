/// 本人改密碼的稽核列只保存識別與固定結果。
pub async fn record_own_password_change(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO analytics_account_audit(id,actor_id,target_id,action,result)
         VALUES($1,$2,$2,'change_own_password','changed')",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
