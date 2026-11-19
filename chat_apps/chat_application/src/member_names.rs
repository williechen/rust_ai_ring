use std::future::Future;

pub trait MemberNames: Sync {
    fn names(&self, ids: Vec<String>) -> impl Future<Output = Result<Vec<String>, String>> + Send;
}

pub async fn online_names(
    store: &impl MemberNames,
    ids: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut names = store.names(ids).await?;
    names.sort();
    Ok(names)
}
