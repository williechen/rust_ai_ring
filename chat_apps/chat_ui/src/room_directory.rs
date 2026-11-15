use crate::room_cards::RoomCards;
use crate::room_list_state::RoomListItem;
use leptos::prelude::*;
use std::collections::BTreeSet;

#[component]
pub(crate) fn RoomDirectory() -> impl IntoView {
    let (rooms, set_rooms) = signal(Vec::<RoomListItem>::new());
    let (query, set_query) = signal(String::new());
    let search_input = NodeRef::<leptos::html::Input>::new();
    let (category, set_category) = signal(None::<String>);
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(None::<String>);

    let (submitted, set_submitted) = signal(String::new());
    let (has_snapshot, set_has_snapshot) = signal(false);
    let failed = RwSignal::new(None::<(String, bool)>);
    let known_labels = RwSignal::new(BTreeSet::<String>::new());
    let revision = RwSignal::new(0u64);
    let refresh = move |search: String, clear: bool| {
        if loading.get_untracked() {
            return;
        }
        let captured = revision.get_untracked();
        set_loading.set(true);
        set_error.set(None);
        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            match crate::room_api::search_rooms(&search).await {
                Ok(next) => {
                    if search.is_empty() {
                        known_labels.set(
                            next.iter()
                                .filter_map(|room| room.category_label.clone())
                                .collect(),
                        );
                    }
                    set_submitted.set(search);
                    set_has_snapshot.set(true);
                    failed.set(None);
                    if clear && revision.get_untracked() == captured {
                        set_query.set(String::new());
                        if let Some(input) = search_input.get_untracked() {
                            let _ = input.focus();
                        }
                    }
                    set_rooms.set(next);
                }
                Err(message) => {
                    failed.set(Some((search, clear)));
                    set_error.set(Some(message));
                }
            }
            set_loading.set(false);
        });

        #[cfg(not(target_arch = "wasm32"))]
        let _ = (
            set_rooms,
            set_submitted,
            set_has_snapshot,
            search,
            clear,
            captured,
            failed,
            search_input,
        );
    };

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| refresh(String::new(), true));

    let labels = known_labels;
    let filtered = Memo::new(move |_| {
        let selected = category.get();
        rooms.with(|rows| {
            rows.iter()
                .cloned()
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
                         on:click=move |_| refresh(String::new(), true)>"重新整理"</button>
                </div>
                <p role="status">{move || format!("最近成功查詢：{}", if !has_snapshot.get() { "尚未查詢".to_owned() } else if submitted.get().is_empty() { "全部名稱".to_owned() } else { submitted.get() })}</p>
                <form class="directory-filters" on:submit=move |event: leptos::ev::SubmitEvent| {
                    event.prevent_default(); refresh(query.get_untracked().trim().to_owned(), false);
                }>
                    <div class="directory-search">
                        <label for="directory-query">"搜尋房間"</label>
                        <input id="directory-query" node_ref=search_input type="search"
                            placeholder="輸入房間名稱" prop:value=move || query.get()
                            on:input=move |event| { revision.update(|n| *n += 1); set_query.set(event_target_value(&event)); } />
                        <button class="directory-secondary" type="submit" disabled=move || loading.get()>"搜尋聊天室"</button>
                        <Show when=move || query.with(|value| !value.is_empty()) || submitted.with(|q| !q.is_empty())>
                            <button class="directory-secondary" type="button" on:click=move |_| {
                                refresh(String::new(), true);
                            }>"清除搜尋"</button>
                        </Show>
                    </div>
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
                </form>
                {move || error.get().map(|message| view! {
                    <div class="directory-error" role="alert">
                        <p>{format!("無法更新聊天室：{message}")}</p>
                         <button class="directory-secondary" type="button" on:click=move |_| { if let Some((q, clear)) = failed.get_untracked() { refresh(q, clear); } }>"重試"</button>
                    </div>
                })}
                {move || {
                    let visible = filtered.get();
                    if !visible.is_empty() {
                        view! { <RoomCards rooms=visible active_room=String::new() /> }.into_any()
                    } else if loading.get() || error.with(Option::is_some) {
                        ().into_any()
                    } else if rooms.with(Vec::is_empty) && submitted.with(String::is_empty) {
                        view! { <p class="directory-empty">"目前沒有公開聊天室。"</p> }.into_any()
                    } else {
                        view! {
                            <div class="directory-empty">
                                <p>"沒有符合條件的聊天室。"</p>
                                <button class="directory-secondary" type="button" on:click=move |_| {
                                    refresh(String::new(), true);
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

#[cfg(test)]
mod day63_directory_tests {
    use super::*;
    #[test]
    fn homepage_submits_a_named_search_and_discloses_snapshot() {
        Owner::new().with(|| {
            let html = view! {<RoomDirectory/>}.to_html();
            assert!(html.contains("搜尋聊天室"));
            assert!(html.contains("最近成功查詢"));
            assert!(html.contains("type=\"submit\""));
        });
    }
}
