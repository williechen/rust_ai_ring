use leptos::prelude::*;

/// 原生錨點只移動控制窗焦點與捲動位置，不呼叫寵物生命週期或檔案命令。
#[component]
pub(crate) fn ControlNavigation() -> impl IntoView {
    view! {
        <nav class="control-navigation" aria-label="寵物控制台功能區">
            {[
                ("pet-interaction", "互動"), ("pet-settings", "設定"),
                ("pet-memory", "記憶"), ("pet-files", "檔案"),
                ("pet-management", "管理"), ("pet-updates", "更新"),
            ].into_iter().map(|(id, label)| view! {
                <a href=format!("#{id}")>{label}</a>
            }).collect_view()}
        </nav>
    }
}
