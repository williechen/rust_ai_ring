#[cfg(target_arch = "wasm32")]
pub(crate) mod service;
pub mod web;
#[cfg(target_arch = "wasm32")]
pub(crate) mod web_service;
