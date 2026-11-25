#[derive(serde::Deserialize)]
pub(crate) struct Login {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub next: String,
}

#[derive(Default, serde::Deserialize)]
pub(crate) struct LoginReturn {
    #[serde(default)]
    pub next: String,
}
