use analytics_ui::navigation::{Navigation, NavigationPage};
use leptos::prelude::*;

/// 權限資料由正式 handler 提供；元件只呈現，不讀取資料庫。
#[component]
pub(crate) fn ManagementShell(
    pages: Vec<NavigationPage>,
    logout_csrf: String,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="analytics-shell">
            <header class="analytics-header"><h1>"Analytics 管理後台"</h1></header>
            <aside class="analytics-sidebar"><details open><summary>"功能導覽"</summary><Navigation pages/></details></aside>
            <main class="analytics-content">{children()}</main>
            <footer class="analytics-footer">
                <p>"來源查詢由伺服器依角色及授權範圍檢查。"</p>
                <form method="post" action="/logout">
                    <input type="hidden" name="csrf" value=logout_csrf/>
                    <button type="submit">"登出"</button>
                </form>
            </footer>
        </div>
    }
}
