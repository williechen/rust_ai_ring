use leptos::prelude::*;

#[component]
fn MemberDocument(title: &'static str, children: Children) -> impl IntoView {
    view! { <html lang="zh-Hant"><head><meta charset="utf-8"/>
    <meta name="viewport" content="width=device-width, initial-scale=1"/>
    <title>{title}</title><style data-project-style="chat">{include_str!("css/chat.css")}</style>
    </head><body><main class="account-workspace"><h1>{title}</h1>{children()}</main></body></html> }
}

pub fn render_signup(complete: bool) -> String {
    let title = if complete {
        "註冊完成"
    } else {
        "會員註冊"
    };
    format!("<!doctype html>{}",view! { <MemberDocument title>
        {if complete {view! { <p>"目前保持登出，請回首頁重新選擇聊天室，再登入。"</p><a href="/">"回首頁"</a> }.into_any()}
        else {view! {
            <form class="account-form" method="post" action="/signup">
                <label for="signup-username">"帳號"</label><input id="signup-username" name="username" required minlength="3" maxlength="32" autocomplete="username"/>
                <label for="signup-email">"Email"</label><input id="signup-email" name="email" type="email" required maxlength="254" autocomplete="email"/>
                <label for="signup-name">"顯示名稱"</label><input id="signup-name" name="display_name" required maxlength="32"/>
                <label for="signup-password">"密碼"</label><input id="signup-password" name="password" type="password" required minlength="8" maxlength="128" autocomplete="new-password"/>
                <button type="submit">"建立會員"</button>
            </form><a href="/signin">"回登入頁"</a>
        }.into_any()}}
    </MemberDocument> }.to_html())
}

pub fn render_password(csrf: String, initial: bool) -> String {
    let title = if initial {
        "首次登入須修改本人密碼"
    } else {
        "修改本人密碼"
    };
    format!("<!doctype html>{}",view! { <MemberDocument title>
        <form class="account-form" method="post" action="/account/password">
            <input type="hidden" name="csrf" value=csrf/>
            <label for="self-current-password">"目前密碼"</label><input id="self-current-password" type="password" name="current_password" autocomplete="current-password" required maxlength="128"/>
            <label for="self-new-password">"新密碼"</label><input id="self-new-password" type="password" name="new_password" autocomplete="new-password" required minlength="8" maxlength="128"/>
            <label for="self-confirm-password">"確認新密碼"</label><input id="self-confirm-password" type="password" name="confirm_password" autocomplete="new-password" required minlength="8" maxlength="128"/>
            <button type="submit">"修改本人密碼"</button>
        </form><a href="/account">"個人管理"</a>
    </MemberDocument> }.to_html())
}
