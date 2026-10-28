pub mod account;

mod chat_socket;

#[cfg(target_arch = "wasm32")]
mod room_api;

mod room_list_state;

#[cfg(all(test, feature = "ssr"))]
mod signin_contract;

use std::rc::Rc;

use chat_domain::{ChatMessage, MessageAuthor};
use chat_socket::ChatSocket;
use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    hooks::{use_params_map, use_query_map},
    path,
};

use room_list_state::{filter_rooms, room_status_text};

#[cfg(any(target_arch = "wasm32", test))]
use room_list_state::refresh_result_view;

#[cfg(any(target_arch = "wasm32"))]
use room_list_state::should_auto_load;

use shared::ClientEvent;

#[cfg(target_arch = "wasm32")]
use shared::ServerEvent;

fn author_label(author: &MessageAuthor) -> String {
    match author {
        MessageAuthor::AnonymousUser => "You".to_string(),
        MessageAuthor::User { user_id } => format!("User {}", user_id),
        MessageAuthor::Ai { persona_id } => format!("AI {}", persona_id),
        MessageAuthor::System => "System".to_string(),
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn apply_room_search(
    previous: Vec<(String, String)>,
    submitted: String,
    result: Result<Vec<(String, String)>, String>,
    set_rows: WriteSignal<Vec<(String, String)>>,
    set_has_snapshot: WriteSignal<bool>,
    set_status: WriteSignal<String>,
    set_loading: WriteSignal<bool>,
) {
    let succeeded = result.is_ok();
    let (next_rows, _retry_query, next_status) = refresh_result_view(previous, submitted, result);
    if succeeded {
        set_has_snapshot.set(true);
    }
    set_rows.set(next_rows);
    set_status.set(next_status);
    set_loading.set(false);
}

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html>
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <title>"Rust AI Chat"</title>
                <HydrationScripts options />
                <style>{include_str!("chat.css")}</style>
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    leptos::mount::hydrate_body(App);
}

#[component]
pub fn App() -> impl IntoView {
    view! {
        <Router>
            <main class="app">
                <h1>
                    <a href="/" rel="external">"Rust AI Chat"</a>
                </h1>
                <Routes fallback=|| view!{
                    <p>"404 - Page Not Found"</p>
                }>
                    <Route path=path!("/") view=HomePage />
                    <Route path=path!("/room/:room_id") view=RoomPage />
                    <Route path=path!("/signin") view=SignInPage />
                </Routes>
            </main>
        </Router>
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SessionEntry {
    Checking,
    SignedOut,
    SignedIn,
    Unavailable,
}

fn session_entry_display(state: SessionEntry) -> (&'static str, &'static str, &'static str) {
    match state {
        SessionEntry::Checking => ("/signin", "登入", "確認登入狀態中"),
        SessionEntry::SignedOut => ("/signin", "登入", ""),
        SessionEntry::SignedIn => ("/account", "管理", ""),
        SessionEntry::Unavailable => ("/signin", "登入", "無法確認登入狀態；可使用登入入口"),
    }
}

#[component]
fn HomePage() -> impl IntoView {
    let (entry, set_entry) = signal(SessionEntry::Checking);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            let next = match room_api::session_status().await {
                Ok(true) => SessionEntry::SignedIn,
                Ok(false) => SessionEntry::SignedOut,
                Err(_) => SessionEntry::Unavailable,
            };
            set_entry.set(next);
        });
    });

    #[cfg(not(target_arch = "wasm32"))]
    let _ = set_entry;

    view! {
        <p>"Welcome to Rust AI Chat!"</p>
        <p>
            <a href=move || session_entry_display(entry.get()).0 rel="external">
                {move || session_entry_display(entry.get()).1}
            </a>
            <span role="status">{move || session_entry_display(entry.get()).2}</span>
        </p>
        <RoomList />
    }
}

// crates/ui/src/lib.rs：完整取代原 SignInPage 函式。
#[component]
fn SignInPage() -> impl IntoView {
    let query = use_query_map();
    let next = move || query.read().get("next").unwrap_or_default();
    view! {
        <section class="sign-in">
            <h2>"使用者登入"</h2>
            <form method="post" action="/login">
                <input type="hidden" name="next" value=next />
                <div>
                    <label for="login-username">"帳號"</label>
                    <input
                        id="login-username"
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

#[component]
fn RoomPage() -> impl IntoView {
    let params = use_params_map();

    move || {
        let room_id = params.read().get("room_id");

        match room_id {
            Some(room_id) => view! {
                <div class="chat-layout">
                    <RoomList />
                    <ChatPage room_id />
                </div>
            }
            .into_any(),
            None => view! { <p>"Invalid Room Id"</p> }.into_any(),
        }
    }
}

#[component]
pub fn ChatPage(room_id: String) -> impl IntoView {
    let (draft, set_draft) = signal(String::new());

    let (connection, set_connection) = signal(String::from("尚未連線"));
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());

    let heading = room_id.clone();

    #[cfg(not(target_arch = "wasm32"))]
    let _ = set_messages;

    #[cfg(target_arch = "wasm32")]
    let socket = {
        let location = web_sys::window().unwrap().location();
        let protocol = if location.protocol().unwrap() == "https:" {
            "wss"
        } else {
            "ws"
        };
        let url = format!("{protocol}://{}/ws/{room_id}", location.host().unwrap());
        match ChatSocket::connect(
            &url,
            move |event| {
                if let ServerEvent::MessageCreated(message) = event {
                    set_messages.update(|messages| messages.push(message));
                }
            },
            move |status| set_connection.set(status.into()),
        ) {
            Ok(socket) => Some(Rc::new(socket)),
            Err(_) => {
                set_connection.set("連線失敗，請重新進房".into());
                None
            }
        }
    };

    #[cfg(not(target_arch = "wasm32"))]
    let socket = Some(Rc::new(ChatSocket::server_stub()));

    let send = {
        let socket = socket.clone();

        move |_| {
            let room_id = room_id.clone();

            let content = draft.get_untracked().trim().to_string();

            if content.is_empty() {
                return;
            }

            let event = ClientEvent::SendMessage { room_id, content };

            match socket
                .as_ref()
                .ok_or_else(|| "尚未連線".to_string())
                .and_then(|socket| socket.send(&event).map_err(|_| "發送失敗".to_string()))
            {
                Ok(()) => {
                    set_draft.set(String::new());
                    set_connection.set("已發送，等待回應...".into());
                }
                Err(_) => set_connection.set("發送失敗，請重新進房".into()),
            }
        }
    };

    view! {
        <section class="conversation">
            <header>
                <h2>{format!("房間 {heading}")}</h2>
                <p role="status">{move || connection.get()}</p>
            </header>
            <p class="history-disclosure" role="note">
                "歷史載入尚未接通：目前只顯示本次連線收到的訊息；重新整理不會載入已儲存訊息。"
            </p>
            <div class="message-history" aria-live="polite">
                {move || {
                    if messages.get().is_empty() {
                        view! {
                            <p class="messages-empty">
                                "目前尚無本次連線收到的訊息"
                            </p>
                        }.into_any()
                    } else {
                        view! {
                            <ul class="messages">
                                {messages.get().into_iter().map(|message| view! {
                                    <li>
                                        <strong>{author_label(&message.author)}": "</strong>
                                        {message.content}
                                    </li>
                                }).collect_view()}
                            </ul>
                        }.into_any()
                    }
                }}
            </div>
            <div>
                <textarea
                    aria-label="聊天訊息"
                    placeholder="Type a message..."
                    prop:value=move || draft.get()
                    on:input=move |event| set_draft.set(event_target_value(&event))
                />

                <button
                    type="button"
                    disabled=move || draft.get().trim().is_empty()
                    on:click=send>"Send"</button>
            </div>
        </section>
    }
}

#[component]
fn RoomList() -> impl IntoView {
    let params = use_params_map();
    let (query, set_query) = signal(String::new());
    let (rows, set_rows) = signal(Vec::<(String, String)>::new());
    let (loading, set_loading) = signal(false);
    let (has_snapshot, set_has_snapshot) = signal(false);
    let (status, set_status) = signal(String::from("請讀取房間列表"));

    let query_room = move || {
        if loading.get_untracked() {
            return;
        }
        set_loading.set(true);
        set_status.set("查詢中…".into());

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let submitted = query.get_untracked();
            let previous = rows.get_untracked();
            let result = room_api::search_rooms(&submitted).await;
            apply_room_search(
                previous,
                submitted,
                result,
                set_rows,
                set_has_snapshot,
                set_status,
                set_loading,
            );
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            set_loading.set(false);
            set_rows.set(Vec::new());
            set_has_snapshot.set(false);
        }
    };

    // chat_apps/chat_ui/src/lib.rs::RoomList，query_room closure 宣告之後、view! 之前
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        if should_auto_load(has_snapshot.get_untracked(), loading.get_untracked()) {
            query_room(); // existing real search_rooms("") -> GET /api/rooms?q=
        }
    });

    view! {
        <section>
            <h2>"房間列表"</h2>
            <input
                type="search"
                aria-label="搜尋房間"
                placeholder="房間名稱"
                prop:value=move || query.get()
                on:input=move |event| set_query.set(event_target_value(&event))
            />

            <button
                type="button"
                disabled=move || loading.get()
                on:click=move |_| { query_room() }
            >"查詢房間"</button>

            <button
                type="button"
                disabled=move || loading.get()
                on:click=move |_| {
                    if loading.get_untracked() { return; }
                    set_loading.set(true);
                    set_status.set("載入中…".into());

                    #[cfg(target_arch = "wasm32")]
                    wasm_bindgen_futures::spawn_local(async move {
                        let previous = rows.get_untracked();
                        let previous_query = query.get_untracked();
                        let result = room_api::list_rooms().await;
                        let succeeded = result.is_ok();
                        let (next_rows, next_query, next_status) =
                            refresh_result_view(previous, previous_query, result);
                        if succeeded {
                            set_has_snapshot.set(true);
                        }
                        set_rows.set(next_rows);
                        set_query.set(next_query);
                        set_status.set(next_status);
                        set_loading.set(false);
                    });

                    #[cfg(not(target_arch = "wasm32"))]
                    { set_loading.set(false); }

            }>"讀取／重新整理房間"</button>
            <div class="room-results" aria-live="polite">
            {move || {
                let all = rows.get();
                let visible = filter_rooms(&all, &query.get());
                let feedback = room_status_text(
                    &status.get(),
                    has_snapshot.get(),
                    all.len(),
                    visible.len(),
                );
                view! {
                    <p class="room-result-feedback" role="status">{feedback}</p>
                    <ul>
                        {visible.into_iter().map(|(id, name)| view! {
                        <li>
                            <a
                                rel="external"
                                aria-current=if params.read().get("room_id").as_deref() == Some(id.as_str()) { Some("page") } else { None }
                                href=format!("/room/{id}")
                            >{name}</a>
                        </li>
                        }).collect_view()}
                    </ul>
                }
            }}
            </div>
        </section>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod day16_layout_tests {
    use super::*;
    #[test]
    fn room_page_has_real_room_identity_and_multiline_composer() {
        let owner = Owner::new();
        let html =
            owner.with(|| view! { <ChatPage room_id="room-fixture".to_owned() /> }.to_html());
        assert!(html.contains("room-fixture"));
        assert!(html.contains("<textarea"));
        assert!(html.contains("尚未連線"));
        assert!(html.contains("歷史載入尚未接通"));
        assert!(!html.contains("Online: 100"));
    }
}

#[cfg(all(test, feature = "ssr"))]
mod day31_account_tests {
    #[test]
    fn account_has_operable_creation_form_and_escaped_feedback() {
        let html = crate::account::render_account(
            "logout-token".into(),
            "create-token".into(),
            "<失敗>".into(),
            "<房名>".into(),
            None,
        );
        assert!(html.contains("action=\"/account/rooms\""));
        assert!(html.contains("name=\"name\""));
        assert!(html.contains("name=\"csrf\" value=\"create-token\""));
        assert!(html.contains("&lt;失敗&gt;"));
        assert!(html.contains("&lt;房名&gt;"));
        assert!(!html.contains("<失敗>"));
    }
}

#[cfg(all(test, feature = "ssr"))]
mod day32_history_disclosure_tests {
    use super::*;
    #[test]
    fn room_explicitly_discloses_missing_history_and_empty_live_messages() {
        let owner = Owner::new();
        let html = owner.with(|| view! { <ChatPage room_id="day32-room".to_owned() /> }.to_html());
        assert!(html.contains("day32-room"));
        assert!(html.contains("歷史載入尚未接通"));
        assert!(html.contains("重新整理不會載入已儲存訊息"));
        assert!(html.contains("目前尚無本次連線收到的訊息"));
        assert!(html.contains("aria-live=\"polite\""));
    }
}

#[cfg(test)]
mod day37_search_tests {
    use super::*;

    #[test]
    fn real_search_completion_updates_the_signals_used_by_the_view() {
        let owner = Owner::new();
        owner.with(|| {
            for result in [
                Ok(vec![("new".to_owned(), "貓房".to_owned())]),
                Ok(vec![]),
                Err("離線".to_owned()),
            ] {
                let old = vec![("old".to_owned(), "原房間".to_owned())];
                let (rows, set_rows) = signal(old.clone());
                let (snapshot, set_snapshot) = signal(true);
                let (status, set_status) = signal("查詢中…".to_owned());
                let (loading, set_loading) = signal(true);
                let expected = result.clone();
                apply_room_search(
                    old.clone(),
                    "貓".to_owned(),
                    result,
                    set_rows,
                    set_snapshot,
                    set_status,
                    set_loading,
                );
                assert!(!loading.get_untracked());
                assert!(snapshot.get_untracked());
                match expected {
                    Ok(value) => {
                        assert_eq!(rows.get_untracked(), value);
                        assert_eq!(
                            status.get_untracked(),
                            if value.is_empty() {
                                "尚無房間"
                            } else {
                                "請選擇房間"
                            }
                        );
                    }
                    Err(_) => {
                        assert_eq!(rows.get_untracked(), old);
                        assert_eq!(status.get_untracked(), "離線，請重試");
                    }
                }
            }
            let (rows, set_rows) = signal(Vec::new());
            let (snapshot, set_snapshot) = signal(false);
            let (_status, set_status) = signal("查詢中…".to_owned());
            let (loading, set_loading) = signal(true);
            apply_room_search(
                Vec::new(),
                String::new(),
                Err("離線".into()),
                set_rows,
                set_snapshot,
                set_status,
                set_loading,
            );
            assert!(rows.get_untracked().is_empty());
            assert!(!snapshot.get_untracked());
            assert!(!loading.get_untracked());
        });
    }
}

#[cfg(test)]
mod day38_room_status_tests {
    use super::room_status_text;

    #[test]
    fn request_progress_and_failure_override_stale_room_feedback() {
        assert_eq!(room_status_text("查詢中…", true, 2, 1), "查詢中…");
        assert_eq!(room_status_text("載入中…", true, 2, 1), "載入中…");
        assert_eq!(
            room_status_text("伺服器：500，請重試", true, 2, 1),
            "伺服器：500，請重試"
        );
        assert_eq!(
            room_status_text("離線，請重試", false, 0, 0),
            "離線，請重試"
        );
    }

    #[test]
    fn success_and_local_filter_show_one_consistent_status() {
        assert_eq!(
            room_status_text("請選擇房間", false, 0, 0),
            "請先讀取房間列表"
        );
        assert_eq!(room_status_text("請選擇房間", true, 0, 0), "尚無房間");
        assert_eq!(
            room_status_text("請選擇房間", true, 2, 0),
            "沒有符合條件的房間"
        );
        assert_eq!(room_status_text("請選擇房間", true, 2, 1), "請選擇房間");
    }
}

#[cfg(test)]
mod day40_home_entry_tests {
    use super::*;
    #[test]
    fn entry_does_not_claim_manage_without_confirmed_session() {
        assert_eq!(session_entry_display(SessionEntry::Checking).1, "登入");
        assert_eq!(session_entry_display(SessionEntry::SignedOut).0, "/signin");
        assert_eq!(
            session_entry_display(SessionEntry::Unavailable).0,
            "/signin"
        );
        assert_eq!(
            session_entry_display(SessionEntry::SignedIn),
            ("/account", "管理", "")
        );
    }
}
