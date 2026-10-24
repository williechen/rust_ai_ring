use leptos::prelude::*;

#[component]
fn LoginView() -> impl IntoView {
    view! {
        <html lang="zh-Hant">
            <head>
                <meta charset="utf-8"/>
                <title>"帳密登入"</title>
            </head>
            <body>
                <main>
                    <h1>"帳密登入"</h1>
                    <form method="post" action="/login">
                        <label for="login-username">"帳號"</label>
                        <input id="login-username" name="username"
                        autocomplete="username" required maxlength="32"/>
                        <label for="login-password">"密碼"</label>
                        <input id="login-password" name="password" type="password"
                        autocomplete="current-password" required maxlength="128"/>
                        <button type="submit">"登入"</button>
                    </form>
                    <a href="/signup">"註冊新帳號"</a>
                    <a href="/">"回公開房間清單"</a>
                </main>
            </body>
        </html>
    }
}

pub fn render_login() -> String {
    format!("<!doctype html>{}", view! { <LoginView/> }.to_html())
}
