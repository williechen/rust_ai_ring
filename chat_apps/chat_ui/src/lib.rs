mod chat_socket;

pub mod account;
pub mod login;
#[cfg(target_arch = "wasm32")]
mod room_api;

use std::rc::Rc;

use chat_domain::{ChatMessage, MessageAuthor};
use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    hooks::use_params_map,
    path,
};

use shared::ClientEvent;

#[cfg(target_arch = "wasm32")]
use shared::ServerEvent;

use chat_socket::ChatSocket;

fn author_label(author: &MessageAuthor) -> String {
    match author {
        MessageAuthor::AnonymousUser => "You".to_string(),
        MessageAuthor::User { user_id } => format!("User {}", user_id),
        MessageAuthor::Ai { persona_id } => format!("AI {}", persona_id),
        MessageAuthor::System => "System".to_string(),
    }
}

fn filter_rooms(rows: &[(String, String)], query: &str) -> Vec<(String, String)> {
    let needle = query.trim().to_lowercase();
    rows.iter()
        .filter(|(_, name)| name.to_lowercase().contains(&needle))
        .cloned()
        .collect()
}

fn room_list_feedback(loaded: bool, total: usize, visible: usize) -> &'static str {
    if !loaded {
        "請先讀取房間列表"
    } else if total == 0 {
        "尚無房間"
    } else if visible == 0 {
        "沒有符合條件的房間"
    } else {
        "請選擇房間"
    }
}

fn refreshed_room_list(
    previous: Vec<(String, String)>,
    result: Result<Vec<(String, String)>, String>,
) -> (Vec<(String, String)>, String) {
    match result {
        Ok(value) => {
            let status = if value.is_empty() {
                "尚無房間"
            } else {
                "請選擇房間"
            };
            (value, status.to_owned())
        }
        Err(message) => (previous, format!("{message}，請重試")),
    }
}

fn refresh_result_view(
    previous: Vec<(String, String)>,
    previous_query: String,
    result: Result<Vec<(String, String)>, String>,
) -> (Vec<(String, String)>, String, String) {
    match result {
        Ok(next) => {
            let status = if next.is_empty() {
                "尚無房間"
            } else {
                "請選擇房間"
            };
            (next, String::new(), status.to_owned())
        }
        Err(error) => (previous, previous_query, format!("{error}，請重試")),
    }
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

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <p>"Welcome to Rust AI Chat!"</p>
        <p>
            <a href="/signin" rel="external">"登入"</a>
        </p>
        <RoomList />
    }
}

// crates/ui/src/lib.rs：完整取代原 SignInPage 函式。
#[component]
fn SignInPage() -> impl IntoView {
    view! {
        <section class="sign-in">
            <h2>"使用者登入"</h2>
            <form method="post" action="/login">
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
            match room_api::search_rooms(&submitted).await {
                Ok(value) => {
                    set_has_snapshot.set(true);
                    set_status.set(
                        if value.is_empty() {
                            if submitted.trim().is_empty() {
                                "尚無房間"
                            } else {
                                "沒有符合條件的房間"
                            }
                        } else {
                            "查詢完成，請選擇房間"
                        }
                        .into(),
                    );
                    set_rows.set(value);
                }
                Err(message) => {
                    set_status.set(format!("查詢失敗：{message}，請重試"));
                }
            }
            set_loading.set(false);
        });
        #[cfg(not(target_arch = "wasm32"))]
        {
            set_loading.set(false);
        }
    };

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
            <p role="status">{move || status.get()}</p>
            <div class="room-results" aria-live="polite">
            {move || {
                let all = rows.get();
                let visible = filter_rooms(&all, &query.get());
                let feedback = room_list_feedback(
                    has_snapshot.get(), all.len(), visible.len()
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

#[cfg(test)]
mod day16_room_search_tests {
    use super::filter_rooms;
    #[test]
    fn day19_empty_success_replaces_previous_rows() {
        let (rows, status) =
            super::refreshed_room_list(vec![("old".into(), "舊房間".into())], Ok(vec![]));
        assert!(rows.is_empty());
        assert_eq!(status, "尚無房間");
    }

    #[test]
    fn day19_failure_preserves_complete_rows_and_message() {
        let old = vec![
            ("one".into(), "同名房間".into()),
            ("two".into(), "同名房間".into()),
        ];
        let (rows, status) = super::refreshed_room_list(old.clone(), Err("讀取失敗：503".into()));
        assert_eq!(rows, old);
        assert_eq!(status, "讀取失敗：503，請重試");
    }

    #[test]
    fn day19_refresh_failure_keeps_previous_room_links() {
        let old = vec![("room_1".into(), "Rust 技術交流".into())];
        let (rows, status) = super::refreshed_room_list(old.clone(), Err("503".into()));
        assert_eq!(rows, old);
        assert!(status.contains("請重試"));

        let (rows, status) =
            super::refreshed_room_list(old, Ok(vec![("room_2".into(), "生活閒聊".into())]));
        assert_eq!(rows[0].0, "room_2");
        assert_eq!(status, "請選擇房間");
    }

    #[test]
    fn search_matches_chinese_names_and_preserves_ids() {
        let rooms = vec![
            ("room_1".to_owned(), "Rust 技術交流".to_owned()),
            ("room_2".to_owned(), "生活閒聊".to_owned()),
        ];
        assert_eq!(filter_rooms(&rooms, " 技術 "), vec![rooms[0].clone()]);
        assert_eq!(filter_rooms(&rooms, ""), rooms);
        assert!(filter_rooms(&rooms, "不存在").is_empty());
    }

    #[test]
    fn day18_query_filter_preserves_room_ids() {
        let rows = vec![
            ("room_1".to_string(), "Rust 技術交流".to_string()),
            ("room_2".to_string(), "生活閒聊".to_string()),
        ];
        assert_eq!(super::filter_rooms(&rows, " 技術 "), vec![rows[0].clone()]);
        assert_eq!(super::filter_rooms(&rows, " "), rows);
    }

    #[test]
    fn day20_refresh_success_reveals_full_snapshot() {
        let old = vec![("a".into(), "Rust".into())];
        let fresh = vec![("a".into(), "Rust".into()), ("b".into(), "Pet".into())];
        let (rows, query, status) =
            super::refresh_result_view(old, "Rust".into(), Ok(fresh.clone()));
        assert_eq!(rows, fresh);
        assert_eq!(query, "");
        assert_eq!(status, "請選擇房間");
        assert_eq!(super::filter_rooms(&rows, &query).len(), 2);
    }

    #[test]
    fn day20_refresh_failure_keeps_query_and_rows() {
        let old = vec![("a".into(), "Rust".into())];
        let (rows, query, status) =
            super::refresh_result_view(old.clone(), "Rust".into(), Err("503".into()));
        assert_eq!(rows, old);
        assert_eq!(query, "Rust");
        assert_eq!(status, "503，請重試");
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

// crates/ui/src/lib.rs：檔尾新增；先跑 Red，後貼 Green。
#[cfg(all(test, feature = "ssr"))]
mod day28_signin_tests {
    use super::*;

    #[test]
    fn day28_leptos_signin_posts_to_existing_login_handler() {
        let html = view! { <SignInPage /> }.to_html();
        assert!(html.contains("<form"));
        assert!(html.contains("method=\"post\""));
        assert!(html.contains("action=\"/login\""));
        assert!(html.contains("name=\"username\""));
        assert!(html.contains("name=\"password\""));
        assert!(html.contains("type=\"password\""));
        assert!(html.contains("autocomplete=\"current-password\""));
        assert!(html.contains("/signup"));
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
mod day33_room_list_feedback_tests {
    use super::room_list_feedback;

    #[test]
    fn distinguish_not_loaded_empty_filtered_and_found() {
        assert_eq!(room_list_feedback(false, 0, 0), "請先讀取房間列表");
        assert_eq!(room_list_feedback(true, 0, 0), "尚無房間");
        assert_eq!(room_list_feedback(true, 3, 0), "沒有符合條件的房間");
        assert_eq!(room_list_feedback(true, 3, 2), "請選擇房間");
    }
}
