use crate::bridge::{self, DirectoryEntry};
use leptos::{prelude::*, task::spawn_local};
use std::collections::{BTreeMap, BTreeSet};

type Path = Vec<String>;

#[derive(Clone)]
struct DirectoryView {
    history: Vec<Path>,
    index: usize,
    entries: Vec<DirectoryEntry>,
    tree: BTreeMap<Path, BTreeSet<String>>,
    skipped: usize,
    loaded: bool,
}

impl Default for DirectoryView {
    fn default() -> Self {
        Self {
            history: vec![Vec::new()],
            index: 0,
            entries: Vec::new(),
            tree: BTreeMap::new(),
            skipped: 0,
            loaded: false,
        }
    }
}

impl DirectoryView {
    fn path(&self) -> &Path {
        &self.history[self.index]
    }
}

#[derive(Clone, Copy)]
enum Navigation {
    Open,
    History(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortField {
    Name,
    Kind,
    Size,
    Modified,
}

impl SortField {
    fn title(self) -> &'static str {
        match self {
            Self::Name => "名稱",
            Self::Kind => "類型",
            Self::Size => "大小",
            Self::Modified => "修改時間",
        }
    }
}

fn extend(path: &[String], name: &str) -> Path {
    let mut next = path.to_vec();
    next.push(name.to_owned());
    next
}

#[component]
pub(crate) fn FileBrowser(
    active: Signal<bool>,
    on_directory: Callback<Option<Vec<String>>>,
) -> impl IntoView {
    let state = RwSignal::new(DirectoryView::default());
    let loading = RwSignal::new(false);
    let attempted = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let icons = RwSignal::new(false);
    let sort = RwSignal::new((SortField::Name, false));
    let selected = RwSignal::new(BTreeSet::<String>::new());
    let folder_form = RwSignal::new(false);
    let folder_name = RwSignal::new(String::new());
    let creating = RwSignal::new(false);
    let created = RwSignal::new(None::<String>);

    let navigate = Callback::new(move |(path, via): (Path, Navigation)| {
        if loading.get_untracked() || creating.get_untracked() {
            return;
        }
        loading.set(true);
        attempted.set(true);
        error.set(None);
        spawn_local(async move {
            match bridge::browse_authorized_directory(&path).await {
                Ok(listing) => {
                    on_directory.run(Some(path.clone()));
                    state.update(|view| {
                        match via {
                            Navigation::History(index) => view.index = index,
                            Navigation::Open if view.path() != &path => {
                                view.history.truncate(view.index + 1);
                                view.history.push(path.clone());
                                view.index += 1;
                            }
                            Navigation::Open => {}
                        }
                        view.tree.insert(
                            path,
                            listing
                                .entries
                                .iter()
                                .filter(|entry| entry.is_directory)
                                .map(|entry| entry.name.clone())
                                .collect(),
                        );
                        view.entries = listing.entries;
                        view.skipped = listing.skipped;
                        view.loaded = true;
                    });
                }
                Err(message) => {
                    on_directory.run(None);
                    state.update(|view| {
                        view.tree.clear();
                        view.entries.clear();
                        view.skipped = 0;
                        view.loaded = false;
                    });
                    error.set(Some(message));
                }
            }
            selected.update(BTreeSet::clear);
            loading.set(false);
        });
    });

    Effect::new(move |_| {
        if active.get() && !attempted.get_untracked() {
            navigate.run((Vec::new(), Navigation::Open));
        }
    });

    let sorted = Memo::new(move |_| {
        let (field, reverse) = sort.get();
        let mut rows = state.with(|view| view.entries.clone());
        rows.sort_by(|left, right| {
            let value = match field {
                SortField::Name => left.name.cmp(&right.name),
                SortField::Kind => right.is_directory.cmp(&left.is_directory),
                SortField::Size => left.size_bytes.cmp(&right.size_bytes),
                SortField::Modified => left.modified_at.cmp(&right.modified_at),
            };
            let value = if reverse { value.reverse() } else { value };
            value.then_with(|| left.name.cmp(&right.name))
        });
        rows
    });

    let go_history = move |back| {
        let next = state.with_untracked(|view| {
            let index = if back {
                view.index.checked_sub(1)?
            } else {
                view.index + 1
            };
            view.history
                .get(index)
                .cloned()
                .map(|path| (path, Navigation::History(index)))
        });
        if let Some(next) = next {
            navigate.run(next);
        }
    };
    let open_folder = move |name: &str| {
        let path = state.with_untracked(|view| extend(view.path(), name));
        navigate.run((path, Navigation::Open));
    };

    view! {
        <div class="file-browser" aria-busy=move || loading.get().to_string()>
            <button type="button" disabled=move || loading.get() || creating.get() || !state.with(|view| view.loaded)
                on:click=move |_| { folder_form.set(true); folder_name.set(String::new()); error.set(None); }>
                "新增資料夾"
            </button>
            <Show when=move || folder_form.get()>
                <fieldset class="file-new-directory" disabled=move || creating.get()>
                    <legend>"建立資料夾確認"</legend>
                    <p>{move || format!("建立位置：授權根目錄/{}", state.with(|view| view.path().join("/")))}</p>
                    <label>"資料夾名稱"
                       <input name="new-directory-name" prop:value=move || folder_name.get()
                            on:input=move |ev| folder_name.set(event_target_value(&ev)) />
                    </label>
                    <button type="button" disabled=move || folder_name.with(|name| name.trim().is_empty())
                        on:click=move |_| {
                            if creating.get_untracked() { return; }
                            let path = state.with_untracked(|view| view.path().clone());
                            let name = folder_name.get_untracked();
                            creating.set(true);
                            created.set(None);
                            error.set(None);
                            spawn_local(async move {
                                let result = bridge::create_authorized_directory(&path, &name).await;
                                creating.set(false);
                                match result {
                                    Ok(()) => {
                                        folder_form.set(false);
                                        created.set(Some(format!("已建立資料夾：{name}")));
                                        navigate.run((path, Navigation::Open));
                                    }
                                    Err(message) => error.set(Some(message)),
                                }
                            });
                        }>"確認建立"</button>
                    <button type="button" on:click=move |_| folder_form.set(false)>"取消"</button>
                </fieldset>
            </Show>
            {move || created.get().map(|message| view! { <p role="status">{message}</p> })}
            <div class="file-tools">
                <div class="file-navigation" role="group" aria-label="目錄導覽">
                    <button type="button" disabled=move || loading.get() || state.with(|view| view.index == 0)
                        on:click=move |_| go_history(true)>"上一頁"</button>
                    <button type="button" disabled=move || loading.get() || state.with(|view| view.index + 1 >= view.history.len())
                        on:click=move |_| go_history(false)>"下一頁"</button>
                    <button type="button" disabled=move || loading.get() || state.with(|view| view.path().is_empty())
                        on:click=move |_| {
                            let mut path = state.with_untracked(|view| view.path().clone());
                            path.pop();
                            navigate.run((path, Navigation::Open));
                        }>"上一層"</button>
                    <button type="button" disabled=move || loading.get() on:click=move |_| {
                        navigate.run((state.with_untracked(|view| view.path().clone()), Navigation::Open));
                    }>"重新整理"</button>
                </div>
                <div role="group" aria-label="內容顯示方式">
                    <button type="button" aria-pressed=move || (!icons.get()).to_string() on:click=move |_| icons.set(false)>"詳細清單"</button>
                    <button type="button" aria-pressed=move || icons.get().to_string() on:click=move |_| icons.set(true)>"圖示"</button>
                </div>
            </div>
            <nav class="file-breadcrumbs" aria-label="目前目錄路徑">
                <button type="button" disabled=move || loading.get() on:click=move |_| navigate.run((Vec::new(), Navigation::Open))>
                    "授權根目錄"
                </button>
                {move || state.with(|view| view.path().clone()).into_iter().enumerate().map(|(index, name)| {
                    let path = state.with_untracked(|view| view.path()[..=index].to_vec());
                    view! {
                        <span aria-hidden="true">"/"</span>
                        <button type="button" disabled=move || loading.get()
                            on:click=move |_| navigate.run((path.clone(), Navigation::Open))>{name}</button>
                    }
                }).collect_view()}
            </nav>
            {move || error.get().map(|message| view! { <p class="control-error" role="alert">{message}</p> })}
            <div class="file-body">
                <nav class="file-tree" aria-label="授權目錄樹">
                    <h3>"目錄"</h3>
                    <button type="button" aria-current=move || state.with(|view| view.path().is_empty()).then_some("location")
                        disabled=move || loading.get() on:click=move |_| navigate.run((Vec::new(), Navigation::Open))>"授權根目錄"</button>
                    {move || state.with(|view| {
                        view.tree.iter().flat_map(|(path, children)| children.iter().map(|name| extend(path, name))).collect::<BTreeSet<_>>()
                    }).into_iter().map(|path| {
                        let name = path.last().cloned().unwrap_or_default();
                        let indent = format!("--tree-depth:{}", path.len().min(5));
                        let active_path = path.clone();
                        view! {
                            <button type="button" class="file-tree-item" style=indent
                                aria-current=move || state.with(|view| view.path() == &active_path).then_some("location")
                                disabled=move || loading.get()
                                on:click=move |_| navigate.run((path.clone(), Navigation::Open))>{name}</button>
                        }
                    }).collect_view()}
                </nav>
                <div class="file-list">
                    <p class="file-count" role="status">{move || state.with(|view| {
                        if loading.get() { "正在讀取目錄…".to_owned() }
                        else if !view.loaded { "目錄內容尚未載入。".to_owned() }
                        else if view.skipped > 0 { format!("{} 個項目 · 選取 {} 個 · 略過 {} 個不可讀或連結項目", view.entries.len(), selected.with(BTreeSet::len), view.skipped) }
                        else if view.entries.is_empty() { "這個目錄是空的。".to_owned() }
                        else { format!("{} 個項目 · 選取 {} 個", view.entries.len(), selected.with(BTreeSet::len)) }
                    })}</p>
                    <Show when=move || state.with(|view| view.loaded && !view.entries.is_empty())>
                        {move || if icons.get() {
                            view! { <ul class="file-icons">
                                <For each=move || sorted.get() key=|entry| entry.name.clone()
                                    children=move |entry| {
                                        let name = entry.name.clone();
                                        view! { <li>
                                            <span class="file-kind" aria-hidden="true">{if entry.is_directory { "▰" } else { "▤" }}</span>
                                            <FileSelection name=entry.name.clone() selected />
                                            {if entry.is_directory {
                                                view! { <button type="button" on:click=move |_| open_folder(&name)>{entry.name}</button> }.into_any()
                                            } else { view! { <span>{entry.name}</span> }.into_any() }}
                                        </li> }
                                    } />
                            </ul> }.into_any()
                        } else {
                            view! { <div class="file-table-scroll" tabindex="0" role="region" aria-label="詳細內容">
                                <table>
                                    <thead><tr><th scope="col">"選取"</th>
                                        {[SortField::Name, SortField::Kind, SortField::Size, SortField::Modified].into_iter().map(|field| view! {
                                            <th scope="col" aria-sort=move || sort.with(|(current, reverse)| (*current == field).then_some(if *reverse { "descending" } else { "ascending" }))>
                                                <button type="button" on:click=move |_| sort.update(|(current, reverse)| {
                                                    *reverse = *current == field && !*reverse;
                                                    *current = field;
                                                })>{field.title()}</button>
                                            </th>
                                        }).collect_view()}
                                    </tr></thead>
                                    <tbody>
                                    <For each=move || sorted.get() key=|entry| (entry.name.clone(), entry.is_directory, entry.size_bytes, entry.modified_at)
                                        children=move |entry| {
                                            let name = entry.name.clone();
                                            let double = name.clone();
                                            view! {
                                                <tr on:dblclick=move |_| { if entry.is_directory { open_folder(&double); } }>
                                                    <td><FileSelection name=entry.name.clone() selected /></td>
                                                    <td>{if entry.is_directory {
                                                        view! { <button type="button" on:click=move |_| open_folder(&name)>{entry.name}</button> }.into_any()
                                                    } else { view! { <span>{entry.name}</span> }.into_any() }}</td>
                                                    <td>{if entry.is_directory { "資料夾" } else { "檔案" }}</td>
                                                    <td>{entry.size_bytes.map(|bytes| format!("{bytes} B")).unwrap_or_else(|| "—".into())}</td>
                                                    <td>{entry.modified_at.map(format_time).unwrap_or_else(|| "—".into())}</td>
                                                </tr>
                                            }
                                        } />
                                    </tbody>
                                </table>
                            </div> }.into_any()
                        }}
                    </Show>
                </div>
            </div>
        </div>
    }
}

#[component]
fn FileSelection(name: String, selected: RwSignal<BTreeSet<String>>) -> impl IntoView {
    let checked = name.clone();
    let label = format!("選取 {name}");
    view! {
        <input type="checkbox" aria-label=label prop:checked=move || selected.with(|names| names.contains(&checked))
            on:change=move |event| selected.update(|names| {
                if event_target_checked(&event) { names.insert(name.clone()); }
                else { names.remove(&name); }
            }) />
    }
}

fn format_time(seconds: u64) -> String {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(seconds as f64 * 1000.0))
            .to_locale_string("zh-TW", &wasm_bindgen::JsValue::UNDEFINED)
            .into()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        seconds.to_string()
    }
}
