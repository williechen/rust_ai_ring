#![cfg_attr(
    all(target_arch = "wasm32", feature = "hydrate"),
    recursion_limit = "256"
)]

pub mod account;
mod chat_socket;
#[cfg(target_arch = "wasm32")]
mod directory_request;
pub mod member_pages;

#[cfg(target_arch = "wasm32")]
mod room_api;

#[cfg(any(target_arch = "wasm32", test))]
mod room_history_state;
mod room_list_state;

mod session_entry;

mod author_display;
mod composer_key;
mod room_cards;
mod room_directory;
mod room_heading;
#[cfg(all(test, feature = "ssr"))]
mod signin_contract;
pub mod signin_form;

use std::rc::Rc;

use chat_domain::ChatMessage;
use chat_socket::ChatSocket;
use leptos::prelude::*;
use leptos_router::{
    components::{Route, Router, Routes},
    hooks::{use_params_map, use_query_map},
    path,
};
#[cfg(all(test, feature = "ssr"))]
use room_cards::RoomCards;
use room_directory::RoomDirectory;
#[cfg(target_arch = "wasm32")]
use room_heading::room_heading;
#[cfg(target_arch = "wasm32")]
use room_history_state::merge_history;
use room_list_state::{RoomListItem, filter_rooms, room_status_text};
use session_entry::{SessionEntry, session_entry_display};

#[cfg(any(target_arch = "wasm32", test))]
use room_list_state::refresh_result_view;

#[cfg(target_arch = "wasm32")]
use room_list_state::should_auto_load;

use shared::ClientEvent;

#[cfg(target_arch = "wasm32")]
use shared::ServerEvent;

use crate::{author_display::author_label, room_list_state::clear_room_search};

#[cfg(any(target_arch = "wasm32", test))]
fn apply_room_search(
    previous: Vec<RoomListItem>,
    submitted: String,
    result: Result<Vec<RoomListItem>, String>,
    set_rows: WriteSignal<Vec<RoomListItem>>,
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
        <html lang="zh-Hant">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <title>"Rust AI Chat"</title>
                <HydrationScripts options />
                <style data-project-style="chat">{include_str!("css/chat.css")}</style>
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
            <header class="site-header">
                <div class="site-header-inner">
                    <a class="site-brand" href="/" rel="external">
                        <span class="site-mark" aria-hidden="true">
                            <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round">
                                <path d="M20 11.5a7.5 7.5 0 0 1-7.5 7.5H5l1.7-3.3a7.5 7.5 0 1 1 13.3-4.2Z" />
                                <path d="M9 11.5h7m-7 3h4" />
                            </svg>
                        </span>
                        "Rust AI Chat"
                    </a>
                    <SiteNavigation />
                </div>
            </header>
            <main class="app" id="main">
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
fn SiteNavigation() -> impl IntoView {
    let (entry, set_entry) = signal(SessionEntry::Checking);
    let (logout_csrf, set_logout_csrf) = signal(None::<String>);

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        wasm_bindgen_futures::spawn_local(async move {
            let next = match room_api::session_status().await {
                Ok(true) => {
                    // 登出權杖由已驗證的會員 Session 取得，未取得時不顯示無效操作。
                    set_logout_csrf.set(room_api::logout_csrf().await.ok());
                    SessionEntry::SignedIn
                }
                Ok(false) => SessionEntry::SignedOut,
                Err(_) => SessionEntry::Unavailable,
            };
            set_entry.set(next);
        });
    });

    #[cfg(not(target_arch = "wasm32"))]
    let _ = (set_entry, set_logout_csrf);

    view! {
         <AuthNavigation state=entry logout_csrf />
    }
}

#[component]
fn AuthNavigation(
    #[prop(into)] state: Signal<SessionEntry>,
    #[prop(into)] logout_csrf: Signal<Option<String>>,
) -> impl IntoView {
    // 導覽節點保持固定，登入狀態只更新文字、連結及登出區域。
    view! {
        <nav aria-label="主要導覽">
            <a href="/" rel="external">"聊天室"</a>
            <a class="site-session" href=move || session_entry_display(state.get()).0 rel="external">
                {move || session_entry_display(state.get()).1}
            </a>
            <Show when=move || state.get() == SessionEntry::SignedIn && logout_csrf.with(Option::is_some)>
                <form method="post" action="/logout">
                    <input type="hidden" name="csrf" value=move || logout_csrf.get().unwrap_or_default()/>
                    <button type="submit">"登出"</button>
                </form>
            </Show>
            <span class="site-status" role="status">{move || session_entry_display(state.get()).2}</span>
        </nav>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <RoomDirectory />
    }
}

#[component]
fn SignInPage() -> impl IntoView {
    let query = use_query_map();

    view! {
        {move || view! { <signin_form::SignInForm
            next=query.read().get("next").unwrap_or_default()
            username=String::new() message=String::new()/>
        }}
    }
}

#[component]
fn RoomPage() -> impl IntoView {
    let params = use_params_map();
    let sidebar_open = RwSignal::new(false);
    let members_open = RwSignal::new(false);

    move || {
        let room_id = params.read().get("room_id");

        match room_id {
            Some(room_id) => view! {
                <div class="chat-layout">
                    <aside class="room-sidebar"
                        data-expanded=move || sidebar_open.get().to_string()>
                        <button class="sidebar-toggle"
                            type="button"
                            aria-expanded=move || sidebar_open.get().to_string()
                            aria-controls="popular-room-panel"
                            on:click=move |_| sidebar_open.update(|open|*open=!*open)>
                                "熱門聊天室"
                        </button>
                        <div class="sidebar-content"
                            id="popular-room-panel">
                            <RoomList />
                        </div>
                    </aside>
                    <ChatPage room_id=room_id.clone() />
                    <aside class="member-sidebar"
                        data-expanded=move || members_open.get().to_string()>
                        <button class="sidebar-toggle"
                            type="button"
                            aria-expanded=move || members_open.get().to_string()
                            aria-controls="room-member-panel"
                            on:click=move |_| members_open.update(|open|*open=!*open)>
                                "在線人員"
                        </button>
                        <div class="member-content"
                            id="room-member-panel">
                            <MemberList room_id />
                        </div>
                    </aside>
                </div>
            }
            .into_any(),
            None => view! { <p>"Invalid Room Id"</p> }.into_any(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn request_history(
    room_id: String,
    set_messages: WriteSignal<Vec<ChatMessage>>,
    set_status: WriteSignal<String>,
) {
    set_status.set("正在載入歷史訊息".into());
    wasm_bindgen_futures::spawn_local(async move {
        match room_api::room_history(&room_id).await {
            Ok(history) => {
                set_messages.update(|rows| *rows = merge_history(std::mem::take(rows), history));
                set_status.set("已載入最近一百則歷史訊息".into());
            }
            Err(error) => set_status.set(format!("{error}；請重試")),
        }
    });
}

#[component]
pub fn ChatPage(room_id: String) -> impl IntoView {
    let (draft, set_draft) = signal(String::new());
    let message_area = NodeRef::<leptos::html::Div>::new();
    let composer = NodeRef::<leptos::html::Textarea>::new();

    let (connection, set_connection) = signal(String::from("尚未連線"));
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let (history_status, set_history_status) = signal(String::from("正在載入歷史訊息"));

    let (room_title, set_room_title) = signal(format!("房間 {room_id}"));
    let name_failed = RwSignal::new(false);
    let name_loading = RwSignal::new(false);
    let name_target = StoredValue::new(room_id.clone());
    let name_alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let name_flag = StoredValue::new(name_alive.clone());
    on_cleanup(move || name_alive.store(false, std::sync::atomic::Ordering::Relaxed));
    let load_name = Callback::new(move |()| {
        if name_loading.get_untracked() {
            return;
        }
        name_loading.set(true);
        #[cfg(target_arch = "wasm32")]
        {
            let requested = name_target.get_value();
            let alive = name_flag.get_value();
            wasm_bindgen_futures::spawn_local(async move {
                let result = room_api::room_name(&requested).await;
                if !alive.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                name_failed.set(result.is_err());
                set_room_title.set(room_heading(&requested, result));
                name_loading.set(false);
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        let _ = (name_target, name_flag, set_room_title);
    });

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| load_name.run(()));

    #[cfg(not(target_arch = "wasm32"))]
    let _ = (set_room_title, set_messages, set_history_status);

    #[cfg(target_arch = "wasm32")]
    request_history(room_id.clone(), set_messages, set_history_status);

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
                    set_messages.update(|rows| {
                        if !rows.iter().any(|m| m.id == message.id) {
                            rows.push(message);
                        }
                    });
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

    let reload_history = {
        let room_id = room_id.clone();
        move |_| {
            #[cfg(target_arch = "wasm32")]
            request_history(room_id.clone(), set_messages, set_history_status);
            #[cfg(not(target_arch = "wasm32"))]
            let _ = &room_id;
        }
    };

    let send = {
        let socket = socket.clone();

        Rc::new(move || {
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
                    // WS 傳送不等於持久化 ACK；維持既有「等待回應」文案。
                    set_draft.set(String::new());
                    #[cfg(target_arch = "wasm32")]
                    if let Some(input) = composer.get_untracked() {
                        let _ = web_sys::HtmlElement::style(&input).set_property("height", "44px");
                    }
                    set_connection.set("已發送，等待回應...".into());
                }
                Err(_) => set_connection.set("發送失敗，請重新進房".into()),
            }
        })
    };

    let send_button = send.clone();
    let send_keyboard = send.clone();

    view! {
    <section class="conversation">
        <header class="conversation-heading">
            <div>
                <h2 title=move ||room_title.get()>{move || room_title.get()}</h2>
                <Show when=move || name_failed.get()>
                    <button class="history-reload"
                        type="button"
                        disabled=move ||name_loading.get()
                        on:click=move |_|load_name.run(())
                    >"重試房間名稱"</button>
                </Show>
                <p role="status">{move || connection.get()}</p>
            </div>
            <div class="conversation-actions">
                <button class="call-action"
                    type="button"
                    aria-label="語音通話（尚未接通）"
                    title="語音通話尚未接通" disabled>
                        "語音"
                </button>
                <button class="call-action"
                    type="button"
                    aria-label="視訊通話（尚未接通）"
                    title="視訊通話尚未接通" disabled>
                        "視訊"
                </button>
                <button class="history-reload"
                    type="button"
                    aria-label="重新載入歷史"
                    on:click=reload_history>
                        "歷史"
                </button>
                <button type="button"
                    class="history-reload"
                    aria-label="跳至最新訊息"
                    on:click=move |_| {
                        #[cfg(target_arch="wasm32")]
                        if let Some(area)=message_area.get_untracked(){ area.set_scroll_top(area.scroll_height()); }
                    }
                >"最新訊息"</button>
                <details class="chat-more">
                    <summary>"更多"</summary>
                    <div class="chat-more-panel">
                        <p>"已定功能・尚未接通"</p>
                        <button type="button" disabled>"文字顏色"</button>
                    </div>
                </details>
            </div>
        </header>
        <p class="history-disclosure"
            role="status">
            {move || history_status.get()}
        </p>
        <div class="message-history"
            node_ref=message_area
            aria-live="polite">
            {move || {
                if messages.get().is_empty() {
                    view! {
                        <p class="messages-empty">
                            "目前尚無訊息"
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
        <div class="chat-composer">
            <textarea node_ref=composer rows="1"
                aria-label="聊天訊息"
                placeholder="輸入訊息"
                prop:value=move || draft.get()
                on:input=move |event| {
                    set_draft.set(event_target_value(&event));
                    #[cfg(target_arch="wasm32")]
                    {
                        let input=event_target::<web_sys::HtmlTextAreaElement>(&event);
                        let _=web_sys::HtmlElement::style(&input).set_property("height","auto");
                        let _=web_sys::HtmlElement::style(&input).set_property("height",&format!("{}px",input.scroll_height().clamp(44,96)));
                    }
                }
                on:keydown=move |event: leptos::ev::KeyboardEvent| {
                    if composer_key::should_send_key(
                        &event.key(), event.shift_key(), event.ctrl_key(),
                        event.alt_key(), event.meta_key(), event.is_composing(),
                        event.key_code(),
                    ) {
                        event.prevent_default();
                        send_keyboard();
                    }
                }
            />
            <button
                type="button"
                disabled=move || draft.get().trim().is_empty()
                on:click=move |_| send_button()>"傳送"</button>
        </div>
        <p class="composer-help">"Enter 傳送；Shift+Enter 換行"
            <span class="composer-ime">"；輸入法選字不傳送。"</span>
        </p>
    </section>
    }
}

#[component]
fn RoomList() -> impl IntoView {
    let params = use_params_map();
    let rows = RwSignal::new(Vec::<room_list_state::PopularRoomItem>::new());
    let loading = RwSignal::new(false);
    let status = RwSignal::new("正在讀取熱門聊天室…".to_owned());

    #[cfg(target_arch = "wasm32")]
    {
        let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let poll_alive = alive.clone();
        let request = Rc::new(move || {
            if !poll_alive.load(std::sync::atomic::Ordering::Relaxed) || loading.get_untracked() {
                return;
            }
            loading.set(true);
            let alive = poll_alive.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = room_api::popular_rooms().await;
                if !alive.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                match result {
                    Ok(next) => {
                        status.set(
                            if next.is_empty() {
                                "目前沒有公開聊天室"
                            } else {
                                "依目前在線人數排序，最多100間"
                            }
                            .into(),
                        );
                        if rows.get_untracked() != next {
                            rows.set(next);
                        }
                    }
                    Err(_) => status.set("暫時無法更新，將自動重試".into()),
                }
                loading.set(false);
            });
        });
        Effect::new(move |_| {
            request();
            let repeat = request.clone();
            let timer = leptos::leptos_dom::helpers::set_interval_with_handle(
                move || repeat(),
                std::time::Duration::from_secs(5),
            )
            .ok();
            on_cleanup(move || {
                if let Some(timer) = timer {
                    timer.clear();
                }
            });
        });
        on_cleanup(move || alive.store(false, std::sync::atomic::Ordering::Relaxed));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = loading;

    view! {
        <section class="popular-rooms"
            aria-label="熱門聊天室">
            <h2>"熱門聊天室"</h2>
            <p role="status">{move ||status.get()}</p>
            <div class="room-results"
                aria-live="polite">
            <ul>
                {move ||rows.get().into_iter().map(|room|view! {
                    <li>
                        <a rel="external"
                            aria-current=if params.read().get("room_id").as_deref()==Some(room.id.as_str()){
                                Some("page")
                            }else{
                                None
                            }
                            href=format!("/room/{}",room.id)>
                            {room.name}
                        </a>
                        <span class="room-online">
                            {format!("{} 人在線",room.online_count)}
                        </span>
                    </li>
                }).collect_view()}
            </ul>
            </div>
        </section>
    }
}

#[component]
fn MemberList(room_id: String) -> impl IntoView {
    let failed = RwSignal::new(false);
    let retry = RwSignal::new(0u64);
    let rows = RwSignal::new(Vec::<String>::new());
    let loading = RwSignal::new(false);
    let status = RwSignal::new("正在讀取在線人員…".to_owned());

    #[cfg(target_arch = "wasm32")]
    {
        let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let poll_alive = alive.clone();
        let request = Rc::new(move || {
            if !poll_alive.load(std::sync::atomic::Ordering::Relaxed) || loading.get_untracked() {
                return;
            }
            loading.set(true);
            let alive = poll_alive.clone();
            let room_id = room_id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let result = room_api::room_members(&room_id).await;
                if !alive.load(std::sync::atomic::Ordering::Relaxed) {
                    return;
                }
                match result {
                    Ok(next) => {
                        failed.set(false);
                        status.set(
                            if next.is_empty() {
                                "目前無在線人員"
                            } else {
                                "目前在線"
                            }
                            .into(),
                        );
                        if rows.get_untracked() != next {
                            rows.set(next);
                        }
                    }
                    Err(error) => {
                        failed.set(true);
                        rows.set(Vec::new());
                        status.set(error);
                    }
                }
                loading.set(false);
            });
        });
        let repeat = request.clone();
        Effect::new(move |_| {
            retry.get();
            request();
        });
        let timer = leptos::leptos_dom::helpers::set_interval_with_handle(
            move || repeat(),
            std::time::Duration::from_secs(5),
        )
        .ok();
        on_cleanup(move || {
            if let Some(timer) = timer {
                timer.clear();
            }
        });
        on_cleanup(move || alive.store(false, std::sync::atomic::Ordering::Relaxed));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (loading, room_id);
    view! {
        <section class="room-members"
            aria-label="在線人員">
            <h2>"在線人員 "
                <span>
                    {move || rows.get().len()}
                </span>
            </h2>
            <p role="status">{move ||status.get()}</p>
            <Show when=move || failed.get()>
                <button type="button" disabled=move || loading.get()
                    on:click=move |_| retry.update(|value| *value=value.wrapping_add(1))
                >"重試在線人員"</button>
            </Show>
            <ul>{move ||rows.get().into_iter().map(|name|
                view! {
                <li>
                    <span class="presence-dot"
                        aria-hidden="true">
                    </span>
                    <button class="private-action"
                        type="button"
                        disabled
                        title="獨立私聊頁尚未接通">
                        <span class="member-name">
                            {name}
                        </span>
                    </button>
                </li>
            }).collect_view()}</ul>
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
        assert!(html.contains("正在載入歷史訊息"));
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
    fn room_discloses_loading_history_and_empty_messages() {
        let owner = Owner::new();
        let html = owner.with(|| view! { <ChatPage room_id="day32-room".to_owned() /> }.to_html());
        assert!(html.contains("day32-room"));
        assert!(html.contains("正在載入歷史訊息"));
        assert!(html.contains("重新載入歷史"));
        assert!(html.contains("目前尚無訊息"));
        assert!(!html.contains("已載入最近一百則歷史訊息"));
        assert!(html.contains("aria-live=\"polite\""));
    }
}

#[cfg(test)]
mod day37_search_tests {
    use crate::room_list_state::test_room;

    use super::*;

    #[test]
    fn real_search_completion_updates_the_signals_used_by_the_view() {
        let owner = Owner::new();
        owner.with(|| {
            for result in [
                Ok(vec![test_room("new", "貓房")]),
                Ok(vec![]),
                Err("離線".to_owned()),
            ] {
                let old = vec![test_room("old", "原房間")];
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

#[cfg(all(test, feature = "ssr"))]
mod day49_room_cards_tests {
    use super::*;
    use crate::room_list_state::test_room;

    #[test]
    fn public_cards_use_real_rooms_and_escape_names() {
        Owner::new().with(|| {
            let html = view! { <RoomCards rooms=vec![RoomListItem { category_label: Some("<程式設計>".into()), ..test_room("room-a", "<房間>") }, test_room("room-b", "第二間")] active_room="room-b".to_owned() /> }.to_html();
            assert_eq!(html.matches(r#"class="room-card""#).count(), 2);
            assert!(html.contains(r#"href="/room/room-a""#));
            assert!(html.contains(r#"href="/room/room-b""#));
            assert!(html.contains("&lt;房間&gt;"));
            assert!(!html.contains("<房間>"));
            assert!(html.contains("&lt;程式設計&gt;"));
            assert!(!html.contains("<程式設計>"));
            assert_eq!(html.matches(r#"aria-current="page""#).count(), 1);
            assert!(!html.contains("在線"));
        });
    }
    #[test]
    fn empty_snapshot_never_manufactures_a_room() {
        Owner::new().with(|| {
            let html = view! { <RoomCards rooms=Vec::new() active_room=String::new() /> }.to_html();
            assert!(!html.contains(r#"class="room-card""#));
            assert!(!html.contains(r#"href="/room/"#));
        });
    }
}

#[cfg(test)]
mod day50_logout_navigation_tests {
    use super::*;

    #[test]
    fn signed_in_navigation_places_real_csrf_logout_after_management() {
        Owner::new().with(|| {
            let html = view! { <AuthNavigation state=SessionEntry::SignedIn logout_csrf=Some("token-hex".to_owned()) /> }.to_html();
            let management = html.find("管理").expect("管理入口");
            let logout = html.find("登出").expect("登出操作");
            assert!(management < logout);
            assert!(html.contains("method=\"post\""));
            assert!(html.contains("action=\"/logout\""));
            assert!(html.contains("name=\"csrf\" value=\"token-hex\""));
            assert!(!html.contains("使用者選單"));
            let signed_out = view! { <AuthNavigation state=SessionEntry::SignedOut logout_csrf=Some("forged".to_owned()) /> }.to_html();
            assert!(!signed_out.contains("action=\"/logout\""));
            let no_token = view! { <AuthNavigation state=SessionEntry::SignedIn logout_csrf=None /> }.to_html();
            assert!(!no_token.contains("action=\"/logout\""));
        });
    }
}

#[cfg(test)]
mod day69_tests {
    use super::*;
    #[test]
    fn room_has_latest_messages_control() {
        let owner = Owner::new();
        owner.with(|| {
            let html = view! {<ChatPage room_id="r".to_string()/>}.to_html();
            assert!(html.contains("最新訊息"));
            assert!(html.contains("跳至最新訊息"));
        });
    }
}
