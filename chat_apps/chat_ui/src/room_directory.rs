use crate::room_cards::RoomCards;
use crate::room_list_state::CategoryItem;
use crate::room_list_state::RoomListItem;
use leptos::prelude::*;

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
    let failed = RwSignal::new(None::<(String, Option<String>, bool)>);
    let known_labels = RwSignal::new(Vec::<CategoryItem>::new());
    let revision = RwSignal::new(0u64);
    let generation = RwSignal::new(0u64);
    let cancelled = RwSignal::new(false);

    #[cfg(target_arch = "wasm32")]
    let active_request = StoredValue::new_local(None::<crate::directory_request::Cancellation>);

    #[cfg(target_arch = "wasm32")]
    on_cleanup(move || {
        if let Some(Some(token)) = active_request.try_get_value() {
            token.cancel();
        }
    });

    let alive = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let request_alive = StoredValue::new(alive.clone());
    on_cleanup(move || alive.store(false, std::sync::atomic::Ordering::Relaxed));

    let refresh = move |search: String, selected: Option<String>, clear: bool| {
        if loading.get_untracked() {
            return;
        }
        let alive = request_alive.get_value();
        let captured = revision.get_untracked();

        generation.update(|value| *value += 1);
        let current_request = generation.get_untracked();

        cancelled.set(false);

        #[cfg(target_arch = "wasm32")]
        let cancellation = crate::directory_request::Cancellation::default();
        #[cfg(target_arch = "wasm32")]
        active_request.set_value(Some(cancellation.clone()));

        set_loading.set(true);
        set_error.set(None);

        #[cfg(target_arch = "wasm32")]
        wasm_bindgen_futures::spawn_local(async move {
            let result = async {
                let categories =
                    crate::room_api::room_categories_cancellable(&cancellation).await?;
                let rooms = crate::room_api::search_rooms_cancellable(
                    &search,
                    selected.as_deref().unwrap_or(""),
                    &cancellation,
                )
                .await?;
                Ok::<_, String>((categories, rooms))
            }
            .await;

            if !alive.load(std::sync::atomic::Ordering::Relaxed) {
                return;
            }

            if generation.get_untracked() != current_request {
                return;
            }

            active_request.set_value(None);

            match result {
                Ok((categories, next)) => {
                    known_labels.set(categories);
                    set_category.set(selected.clone());
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
                    failed.set(Some((search, selected, clear)));
                    set_error.set(Some(message));
                }
            }

            set_loading.set(false);
        });

        #[cfg(not(target_arch = "wasm32"))]
        let _ = (
            current_request,
            set_category,
            set_rooms,
            set_submitted,
            set_has_snapshot,
            search,
            selected,
            alive,
            clear,
            captured,
            failed,
            search_input,
        );
    };

    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| refresh(String::new(), None, true));

    let cancel = move |_| {
        #[cfg(target_arch = "wasm32")]
        if let Some(token) = active_request.get_value() {
            token.cancel();
            active_request.set_value(None);
        }
        generation.update(|value| *value += 1);
        set_loading.set(false);
        set_error.set(None);
        failed.set(None);
        cancelled.set(true);
    };

    let labels = known_labels;
    let filtered = Memo::new(move |_| rooms.get());

    view! {
        <div class="room-directory">
            <section class="directory-hero"
                aria-labelledby="directory-title">
                <p class="directory-eyebrow">"公開目錄"</p>
                <h1 id="directory-title">"探索聊天室"</h1>
                <p>"瀏覽房間，找到想加入的對話。選擇房間後登入即可繼續。"</p>
            </section>
            <section class="directory-panel"
                aria-labelledby="directory-results-title"
                aria-busy=move || loading.get().to_string()>
                <div class="directory-toolbar">
                    <div>
                        <h2 id="directory-results-title">"聊天室"</h2>
                        <p class="directory-count" role="status">
                            {move || if loading.get() {
                                "正在載入聊天室…".to_owned()
                            }  else if !has_snapshot.get(){
                                "清單尚未取得".to_owned()
                            } else {
                                format!("顯示 {} 間聊天室", filtered.read().len())
                            }}
                        </p>
                    </div>
                    <button class="directory-secondary"
                        type="button"
                        disabled=move || loading.get()
                        on:click=move |_| refresh(String::new(), category.get_untracked(), true)>"重新整理"</button>
                </div>
                <Show when=move || loading.get()>
                    <button class="directory-secondary"
                        type="button"
                        on:click=cancel>"取消查詢"
                    </button>
                </Show>
                <Show when=move || cancelled.get()>
                    <p class="directory-cancelled"
                        role="status">{move ||if has_snapshot.get(){
                            "已取消查詢，保留上次清單及目前輸入。"
                        }else{
                            "已取消查詢，尚未取得清單；可重新整理或搜尋。"
                        }}
                    </p>
                </Show>
                <p role="status">{move || format!("最近成功查詢：{}",
                    if !has_snapshot.get() {
                        "尚未查詢".to_owned()
                    } else if submitted.get().is_empty() {
                        "全部名稱".to_owned()
                    } else {
                        submitted.get()
                    })}
                </p>
                <form class="directory-filters"
                    on:submit=move |event: leptos::ev::SubmitEvent| {
                        event.prevent_default();
                        refresh(query.get_untracked().trim().to_owned(),category.get_untracked(), false);
                    }
                >
                    <div class="directory-search">
                        <label for="directory-query">"搜尋房間"</label>
                        <input id="directory-query"
                            node_ref=search_input
                            type="search"
                            placeholder="輸入房間名稱"
                            prop:value=move || query.get()
                            on:input=move |event| {
                                revision.update(|n| *n += 1);
                                set_query.set(event_target_value(&event));
                            } />
                        <button class="directory-secondary"
                            type="submit"
                            disabled=move || loading.get()>"搜尋聊天室"
                        </button>
                        <Show when=move || query.with(|value| !value.is_empty()) || submitted.with(|q| !q.is_empty())>
                            <button class="directory-secondary"
                                type="button" on:click=move |_| {
                                    refresh(String::new(), category.get_untracked(), true);
                                }
                            >"清除搜尋"</button>
                        </Show>
                    </div>
                    <fieldset class="directory-categories">
                        <legend>"分類"</legend>
                        <div>
                            <button type="button"
                                aria-pressed=move || category.with(Option::is_none).to_string()
                                disabled=move || loading.get()
                                on:click=move |_| refresh(query.get_untracked().trim().to_owned(),None,false)
                            >"全部"</button>
                            <For each=move || labels.get()
                                key=|item|item.code.clone()
                                children=move |item| {
                                    let selected=item.code.clone();
                                    let pressed=item.code.clone();
                                    view! {
                                        <button type="button"
                                            data-category=item.code.clone()
                                            aria-label=format!("分類：{}（{}）",item.label,item.code)
                                            aria-pressed=move || category.with(|value|value.as_ref()==Some(&pressed)).to_string()
                                            disabled=move || loading.get()
                                            on:click=move |_|refresh(query.get_untracked().trim().to_owned(),Some(selected.clone()),false)
                                        >{item.label.clone()}</button>
                                    }
                            } />
                        </div>
                    </fieldset>
                </form>
                {move || error.get().map(|message| view! {
                    <div class="directory-error"
                        role="alert">
                        <p>{format!("無法更新聊天室：{message}")}</p>
                        <button class="directory-secondary"
                            type="button"
                            on:click=move |_| {
                                if let Some((q, c, clear)) = failed.get_untracked() {
                                    refresh(q, c, clear);
                                }
                            }>"重試"</button>
                    </div>
                })}
                {move || {
                    let visible = filtered.get();
                    if !visible.is_empty() {
                        view! { <RoomCards rooms=visible active_room=String::new() /> }.into_any()
                    } else if loading.get() || error.with(Option::is_some) {
                        ().into_any()
                    } else if !has_snapshot.get(){
                        ().into_any()
                    } else if rooms.with(Vec::is_empty) && submitted.with(String::is_empty) && category.with(Option::is_none) {
                        view! { <p class="directory-empty">"目前沒有公開聊天室。"</p> }.into_any()
                    } else {
                        view! {
                            <div class="directory-empty">
                                <p>"沒有符合條件的聊天室。"</p>
                                <button class="directory-secondary"
                                    type="button" on:click=move |_| {
                                        refresh(String::new(), None, true);
                                    }
                                >"清除篩選"</button>
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
