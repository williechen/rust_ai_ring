pub struct SqliteCageQuery(pub sqlx::SqlitePool);
impl pet_application::cage_query::CageQuery for SqliteCageQuery {
    async fn read(&self, id: &str) -> Result<bool, String> {
        crate::read_caged(&self.0, id).await
    }
}
