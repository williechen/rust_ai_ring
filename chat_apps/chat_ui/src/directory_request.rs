use std::{cell::Cell, rc::Rc};
use wasm_bindgen::{JsCast, closure::Closure};
use wasm_bindgen_futures::JsFuture;

struct Deadline {
    window: web_sys::Window,
    controller: web_sys::AbortController,
    handle: i32,
    _callback: Closure<dyn FnMut()>,
    expired: Rc<Cell<bool>>,
}
impl Drop for Deadline {
    fn drop(&mut self) {
        self.window.clear_timeout_with_handle(self.handle);
        self.controller.abort();
    }
}
impl Deadline {
    fn new(window: web_sys::Window) -> Result<Self, String> {
        let controller = web_sys::AbortController::new().map_err(|_| "無法建立請求取消控制")?;
        let expired = Rc::new(Cell::new(false));
        let flag = expired.clone();
        let abort = controller.clone();
        let callback = Closure::new(move || {
            flag.set(true);
            abort.abort();
        });
        let handle = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                callback.as_ref().unchecked_ref(),
                15000,
            )
            .map_err(|_| "無法建立讀取期限")?;
        Ok(Self {
            window,
            controller,
            handle,
            _callback: callback,
            expired,
        })
    }
    fn error(&self) -> String {
        if self.expired.get() {
            "讀取逾時（15秒），請重試".into()
        } else {
            "讀取失敗，請重試".into()
        }
    }
}
pub async fn text(url: &str) -> Result<String, String> {
    let scope = Deadline::new(web_sys::window().ok_or("無法取得瀏覽器")?)?;
    let options = web_sys::RequestInit::new();
    options.set_cache(web_sys::RequestCache::NoStore);
    options.set_signal(Some(&scope.controller.signal()));
    let value = JsFuture::from(scope.window.fetch_with_str_and_init(url, &options))
        .await
        .map_err(|_| scope.error())?;
    let response: web_sys::Response = value.dyn_into().map_err(|_| "回應格式無效")?;
    if !response.ok() {
        return Err(format!("讀取失敗：HTTP {}", response.status()));
    }
    JsFuture::from(response.text().map_err(|_| "讀取本文失敗")?)
        .await
        .map_err(|_| scope.error())?
        .as_string()
        .ok_or_else(|| "回應不是文字".into())
}
