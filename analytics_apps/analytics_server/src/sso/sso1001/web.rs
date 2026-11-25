use leptos::prelude::*;

#[component]
fn LoginView(username: String, message: String, next: String) -> impl IntoView {
    view! {
        <html lang="zh-Hant">
            <head>
                <meta charset="utf-8"/>
                <title>"Analytics 帳密登入"</title><meta name="viewport" content="width=device-width, initial-scale=1"/><style data-project-style="analytics">{include_str!("../../css/dashboard.css")}</style>
            </head>
            <body>
                <main class="analytics-auth">
                    <h1>"Analytics 帳密登入"</h1>
                    <p role="alert">{message}</p>
                    <form method="post" action="/login">
                        <input type="hidden" name="next" value=next/>
                        <div>
                            <label for="analytics-username">"帳號"</label>
                            <input id="analytics-username" name="username" value=username
                                autocomplete="username" required/>
                        </div>
                        <div>
                            <label for="analytics-password">"密碼"</label>
                            <input id="analytics-password" name="password"
                                type="password" autocomplete="current-password" required/>
                        </div>
                        <button type="submit">"登入"</button>
                    </form>
                </main>
            </body>
        </html>
    }
}

pub fn render_login() -> String {
    render_login_result("", "")
}

pub fn render_login_result(username: &str, message: &str) -> String {
    render_login_with_next(username, message, "")
}

pub(crate) fn render_login_with_next(username: &str, message: &str, next: &str) -> String {
    let next = analytics_application::return_target::checked(next).unwrap_or("");
    format!("<!doctype html>{}",view!{<LoginView username=username.to_owned() message=message.to_owned() next=next.to_owned()/>}.to_html())
}
