use crate::room_cards::RoomCards;
use crate::room_list_state::{RoomListItem, filter_rooms};
use leptos::prelude::*;
use std::collections::BTreeSet;

#[component]
pub(crate) fn RoomDirectory() -> impl IntoView {
    let (rooms, set_rooms) = signal(Vec::<RoomListItem>::new());
    let (query, set_query) = signal(String::new());
    let (category, set_category) = signal(None::<String>);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);

    let refresh = move || {
        set_loading.set(true);
        set_error.set(None);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            match crate::room_api::list_rooms().await {
                Ok(next) => {
                    if let Some(selected) = category.get_untracked() {
                        if !next
                            .iter()
                            .any(|room| room.category_label.as_ref() == Some(&selected))
                        {
                            set_category.set(None);
                        }
                    }
                    set_rooms.set(next);
                }
                Err(message) => set_error.set(Some(message)),
            }
            set_loading.set(false);
        });

        #[cfg(not(target_arch = "wasm32"))]
        let _ = set_rooms;
    };

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| refresh());

    let labels = Memo::new(move |_| {
        rooms.with(|rows| {
            rows.iter()
                .filter_map(|room| room.category_label.clone())
                .collect::<BTreeSet<_>>()
        })
    });
    let filtered = Memo::new(move |_| {
        let selected = category.get();
        rooms.with(|rows| {
            filter_rooms(rows, &query.get())
                .into_iter()
                .filter(|room| selected.is_none() || room.category_label == selected)
                .collect::<Vec<_>>()
        })
    });

    view! {
        <div class="room-directory">
            <section class="directory-hero" aria-labelledby="directory-title">
                <p class="directory-eyebrow">"公開目錄"</p>
                <h1 id="directory-title">"探索聊天室"</h1>
                <p>"瀏覽房間，找到想加入的對話。選擇房間後登入即可繼續。"</p>
            </section>
            <section class="directory-panel" aria-labelledby="directory-results-title" aria-busy=move || loading.get().to_string()>
                <div class="directory-toolbar">
                    <div>
                        <h2 id="directory-results-title">"聊天室"</h2>
                        <p class="directory-count" role="status">
                            {move || if loading.get() {
                                "正在載入聊天室…".to_owned()
                            } else {
                                format!("顯示 {} 間聊天室", filtered.read().len())
                            }}
                        </p>
                    </div>
                    <button class="directory-secondary" type="button" disabled=move || loading.get()
                        on:click=move |_| refresh()>"重新整理"</button>
                </div>
                <div class="directory-filters">
                    <label class="directory-search">
                        <span>"搜尋房間"</span>
                        <input type="search" placeholder="輸入房間名稱" prop:value=move || query.get()
                            on:input=move |event| set_query.set(event_target_value(&event)) />
                    </label>
                    <fieldset class="directory-categories">
                        <legend>"分類"</legend>
                        <div>
                            <button type="button" aria-pressed=move || category.with(Option::is_none).to_string()
                                on:click=move |_| set_category.set(None)>"全部"</button>
                            <For each=move || labels.get() key=|label| label.clone() children=move |label| {
                                let selected = label.clone();
                                let pressed = label.clone();
                                view! {
                                    <button type="button" aria-pressed=move || category.with(|value| value.as_ref() == Some(&pressed)).to_string()
                                        on:click=move |_| set_category.set(Some(selected.clone()))>{label}</button>
                                }
                            } />
                        </div>
                    </fieldset>
                </div>
                {move || error.get().map(|message| view! {
                    <div class="directory-error" role="alert">
                        <p>{format!("無法更新聊天室：{message}")}</p>
                        <button class="directory-secondary" type="button" on:click=move |_| refresh()>"重試"</button>
                    </div>
                })}
                {move || {
                    let visible = filtered.get();
                    if !visible.is_empty() {
                        view! { <RoomCards rooms=visible active_room=String::new() /> }.into_any()
                    } else if loading.get() || error.with(Option::is_some) {
                        ().into_any()
                    } else if rooms.with(Vec::is_empty) {
                        view! { <p class="directory-empty">"目前沒有公開聊天室。"</p> }.into_any()
                    } else {
                        view! {
                            <div class="directory-empty">
                                <p>"沒有符合條件的聊天室。"</p>
                                <button class="directory-secondary" type="button" on:click=move |_| {
                                    set_query.set(String::new());
                                    set_category.set(None);
                                }>"清除篩選"</button>
                            </div>
                        }.into_any()
                    }
                }}
            </section>
        </div>
    }
}
