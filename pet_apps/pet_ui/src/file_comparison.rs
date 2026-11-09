use crate::bridge::{self, ComparisonRow, DirectoryComparison};
use crate::scan_generation::accept_scan_result;
use leptos::{prelude::*, task::spawn_local};
use std::collections::BTreeMap;

#[derive(Default)]
struct PreviewTree {
    folders: BTreeMap<String, PreviewTree>,
    files: BTreeMap<String, ComparisonRow>,
}

fn build_tree(rows: Vec<ComparisonRow>) -> PreviewTree {
    let mut root = PreviewTree::default();
    for row in rows {
        let Some((filename, path)) = row.proposed.split_last() else {
            continue;
        };
        let mut folder = &mut root;
        for segment in path {
            folder = folder.folders.entry(segment.clone()).or_default();
        }
        folder.files.insert(filename.clone(), row);
    }
    root
}

fn render_tree(tree: PreviewTree, depth: usize) -> AnyView {
    view! {
        <ul class="comparison-tree">
            {tree.folders.into_iter().map(|(name, child)| {
                let title = if depth == 0 && name.len() == 64 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
                    format!("SHA256 {name}")
                } else { name };
                view! { <li><details open=depth < 2>
                    <summary>{title}</summary>
                    {render_tree(child, depth + 1)}
                </details></li> }
            }).collect_view()}
            {tree.files.into_iter().map(|(name, row)| view! { <li class="comparison-leaf">
                <strong>{name}</strong>
                <small>{format!("原位置 {} · {} B", row.original.join(" / "), row.size_bytes)}</small>
            </li> }).collect_view()}
        </ul>
    }.into_any()
}

#[component]
pub(crate) fn FileComparison(directory: ReadSignal<Option<Vec<String>>>) -> impl IntoView {
    let (result, set_result) = signal(None::<DirectoryComparison>);
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let (confirmed, set_confirmed) = signal(false);
    let generation = RwSignal::new(0_u64);
    let reset = move || {
        generation.update(|value| *value = value.wrapping_add(1));
        set_result.set(None);
        set_loading.set(false);
        set_confirmed.set(false);
        set_error.set(None);
    };
    Effect::new(move |_| {
        let _ = directory.get();
        reset();
    });
    let scan = move || {
        let Some(path) = directory.get_untracked() else {
            return;
        };
        reset();
        let ticket = generation.get_untracked();
        set_loading.set(true);
        spawn_local(async move {
            let incoming = bridge::compare_authorized_directory(&path).await;
            if !accept_scan_result(ticket, generation.get_untracked())
                || directory.get_untracked().as_ref() != Some(&path)
            {
                return;
            }
            set_loading.set(false);
            match incoming {
                Ok(listing) => set_result.set(Some(listing)),
                Err(message) => set_error.set(Some(message)),
            }
        });
    };

    view! {
        <section class="file-comparison" aria-labelledby="file-comparison-title" aria-busy=move || loading.get().to_string()>
            <div class="control-section-heading">
                <div>
                    <h3 id="file-comparison-title">"整理與重複目錄比對"</h3>
                    <p>"掃描目前目錄，將原始清單與兩種處理結果一起預覽。"</p>
                </div>
                <button class="control-primary" type="button"
                    disabled=move || loading.get() || directory.with(Option::is_none)
                    on:click=move |_| scan()>"產生比對預覽"</button>
            </div>
            <Show when=move || loading.get()><p role="status">"正在掃描與核對 SHA256…"</p></Show>
            {move || error.get().map(|message| view! { <p class="control-error" role="alert">{message}</p> })}
            <Show when=move || loading.get() || result.with(Option::is_some)>
                <button type="button" on:click=move |_| reset()>"取消預覽"</button>
            </Show>
            {move || result.get().map(|listing| {
                let total = listing.original.len();
                let skipped = listing.skipped;
                let duplicate_total = listing.duplicates.len();
                view! {
                    <section class="comparison-original" aria-labelledby="comparison-original-title">
                        <h4 id="comparison-original-title">"原始清單"</h4>
                        <p role="status">{format!("{} 個檔案，{} 筆略過或無法讀取。", total, skipped)}</p>
                        {if listing.original.is_empty() {
                            view! { <p>"這個目錄沒有可預覽的檔案。"</p> }.into_any()
                        } else {
                            view! { <ul>{listing.original.into_iter().map(|row| view! {
                                <li><span>{row.original.join(" / ")}</span><small>{format!("{} B", row.size_bytes)}</small></li>
                            }).collect_view()}</ul> }.into_any()
                        }}
                    </section>
                    <div class="comparison-outcomes">
                        <section class="comparison-output" aria-labelledby="comparison-organized-title">
                            <h4 id="comparison-organized-title">"整理後目錄樹"</h4>
                            <p>"按副檔名分組，保留相對路徑。"</p>
                            {if listing.organized.is_empty() {
                                view! { <p>"沒有待整理的檔案。"</p> }.into_any()
                            } else { render_tree(build_tree(listing.organized), 0) }}
                        </section>
                        <section class="comparison-output" aria-labelledby="comparison-duplicates-title">
                            <h4 id="comparison-duplicates-title">"重複檔目錄樹"</h4>
                            <p>"只列出 SHA256 相同的檔案。"</p>
                            {if duplicate_total == 0 {
                                view! { <p>"沒有內容相同的檔案。"</p> }.into_any()
                            } else { render_tree(build_tree(listing.duplicates), 0) }}
                        </section>
                    </div>
                    <button type="button" on:click=move |_| set_confirmed.set(true)>"確認已檢視"</button>
                    <Show when=move || confirmed.get()>
                        <p role="status">"已確認這次預覽。檔案維持原狀。"</p>
                    </Show>
                }
            })}
        </section>
    }
}
