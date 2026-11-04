use leptos::prelude::*;

#[component]
pub(crate) fn SignInForm(next: String, username: String, message: String) -> impl IntoView {
    view! {
        <section class="sign-in">
            <h2>"使用者登入"</h2>
            <p role="alert">{message}</p>
            <form method="post" action="/login">
                <input type="hidden" name="next" value=next />
                <div>
                    <label for="login-username">"帳號"</label>
                    <input
                        id="login-username"
                        value=username
                        name="username"
                        type="text"
                        autocomplete="username"
                        required
                        maxlength="32"
                    />
                </div>
                <div>
                    <label for="login-password">"密碼"</label>
                    <input
                        id="login-password"
                        name="password"
                        type="password"
                        autocomplete="current-password"
                        required
                        maxlength="128"
                    />
                </div>
                <button type="submit">"登入"</button>
            </form>
            <p>
                <a href="/signup" rel="external">"建立會員"</a>
            </p>
            <p>
                <a href="/" rel="external">"回房間清單"</a>
            </p>
        </section>
    }
}

/// 與正常入口共用表單；密碼沒有可回填的參數。
pub fn render_login_failure(username: String, next: String) -> String {
    format!("<!doctype html>{}", view! {
        <html lang="zh-Hant"><head><meta charset="utf-8"/><title>"登入未完成"</title></head>
        <body><main><SignInForm username next message="帳號或密碼不正確，請重新輸入。".to_string()/></main></body></html>
    }.to_html())
}
