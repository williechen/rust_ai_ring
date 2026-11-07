use leptos::prelude::*;

#[component]
fn AccountView(
    logout_csrf: String,
    create_csrf: String,
    message: String,
    name: String,
    room_id: Option<String>,
) -> impl IntoView {
    view! {
        <html lang="zh-Hant">
            <head><meta charset="utf-8"/><title>"個人管理"</title></head>
            <body><main>
                <h1>"個人管理"</h1>
                <p role="status">{message}</p>
                {room_id.map(|id| view! {
                    <p><a href=format!("/room/{id}")>"進入剛建立的房間"</a></p>
                })}
                <form method="post" action="/account/rooms">
                    <input type="hidden" name="csrf" value=create_csrf/>
                    <label for="room-name">"聊天室名稱"</label>
                    <input id="room-name" name="name" value=name required maxlength="200"/>
                    <button type="submit">"建立聊天室"</button>
                </form>
                <form method="post" action="/logout">
                    <input type="hidden" name="csrf" value=logout_csrf/>
                    <button type="submit">"登出"</button>
                </form>
                <a href="/">"回公開房間清單"</a>
            </main></body>
        </html>
    }
}

pub fn render_account(
    logout_csrf: String,
    create_csrf: String,
    message: String,
    name: String,
    room_id: Option<String>,
) -> String {
    format!(
        "<!doctype html>{}",
        view! {
            <AccountView logout_csrf create_csrf message name room_id/>
        }
        .to_html()
    )
}

#[component]
fn ProfileView(csrf: String) -> impl IntoView {
    view! {
        <html lang="zh-Hant">
            <head><meta charset="utf-8"/><title>"修改本人資料"</title></head>
            <body>
                <main>
                    <h1>"修改本人資料"</h1>
                    <form method="post" action="/account/profile">
                        <input type="hidden" name="csrf" value=csrf/>
                        <label for="profile-display-name">
                            "顯示名稱"
                            <input id="profile-display-name" name="display_name"
                                required maxlength="32"/>
                        </label>
                        <label for="profile-email">
                            "Email"
                            <input id="profile-email" type="email" name="email"
                                required maxlength="254"/>
                        </label>
                        <button type="submit">"儲存本人資料"</button>
                    </form>
                    <a href="/account">"個人管理"</a>
                </main>
            </body>
        </html>
    }
}

/// 由 Leptos SSR 統一處理屬性跳脫，避免以字串拼接會員表單。
pub fn render_profile(csrf: String) -> String {
    format!("<!doctype html>{}", view! { <ProfileView csrf/> }.to_html())
}
