use leptos::prelude::*;

#[component]
pub(crate) fn RefreshReportLink(href: String) -> impl IntoView {
    view! { <a class="report-refresh" href=href>"重新整理報表"</a> }
}

#[component]
pub(crate) fn FirstReportPageLink(href: String) -> impl IntoView {
    view! { <a href=href>"回到第一頁"</a> }
}

#[component]
pub(crate) fn InvalidDateForm(
    source: String,
    event_type: String,
    page_size: usize,
    start: String,
    end: String,
) -> impl IntoView {
    view! {
        <form class="report-filters"
            action="/dashboard"
            method="get"
            aria-label="修正日期查詢">
            <input type="hidden" name="source" value=source/>
            <input type="hidden" name="page_size" value=page_size.to_string()/>
            <input type="hidden" name="page" value="1"/>
            <div class="report-field">
                <label for="event-type">"事件類型（模糊查詢）"</label>
                <input id="event-type" name="event_type" value=event_type/>
            </div>
            <div class="report-field">
                <label for="report-start">"起日"</label>
                <input id="report-start" type="date" name="start" value=start/>
            </div>
            <div class="report-field">
                <label for="report-end">"迄日"</label>
                <input id="report-end" type="date" name="end" value=end/>
            </div>
            <div class="report-actions">
                <button class="report-primary" type="submit">"修正並查詢"</button>
            </div>
        </form>
    }
}
