pub struct SqliteMemberNames(pub sqlx::SqlitePool);
impl chat_application::member_names::MemberNames for SqliteMemberNames {
    async fn names(&self, ids: Vec<String>) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        for id in ids {
            let name: Option<String> = sqlx::query_scalar(
                r#"SELECT user_name 
                FROM mbr_user 
                WHERE id=? 
                    AND user_status='active'"#,
            )
            .bind(id)
            .fetch_optional(&self.0)
            .await
            .map_err(|_| "成員名稱查詢失敗")?;
            if let Some(name) = name {
                names.push(name);
            }
        }
        Ok(names)
    }
}
