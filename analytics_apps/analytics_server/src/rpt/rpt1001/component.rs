use leptos::prelude::*;

#[component]
pub(crate) fn RefreshReportLink(href: String) -> impl IntoView {
    view! { <a class="report-refresh" href=href>"重新整理報表"</a> }
}

#[component]
pub(crate) fn FirstReportPageLink(href: String) -> impl IntoView {
    view! { <a href=href>"回到第一頁"</a> }
}