use analytics_application::daily_report::DailyReport;
use leptos::prelude::*;
#[component]
pub(crate) fn DailyChart(report: DailyReport) -> impl IntoView {
    let maximum = report
        .rows
        .iter()
        .map(|(_, n)| *n)
        .max()
        .unwrap_or(0)
        .max(1);
    let unknowable = report.unknown_dates > 0 && report.rows.iter().all(|(_, n)| *n == 0);
    let caption = format!(
        "{} 至 {}（迄日不含）· UTC · 依事件發生日期",
        report.start, report.end
    );
    view! {
      <section class="report-card daily-report" aria-labelledby="daily-chart-title">
       <h3 id="daily-chart-title">"每日事件趨勢"</h3><p>{caption}</p>
       { (report.unknown_dates>0).then(||view!{<p class="daily-warning" role="status">{format!("此來源／事件類型共有 {} 筆缺少發生時間，日期無法判定，未納入圖表；此圖不是完整期間統計。",report.unknown_dates)}</p>}) }
       {if unknowable {view!{<p>"目前沒有可依發生日期分析的資料，不以接收日期或假零值補圖。"</p>}.into_any()} else {view!{
        <figure class="daily-figure" aria-label="每日已提交事件數圖表"><div class="daily-axis">{format!("每日事件數（筆）· 刻度 0–{maximum}")}</div>
        <div class="daily-bars" style=format!("grid-template-columns:repeat({},minmax(32px,1fr))",report.rows.len())>
          {report.rows.iter().map(|(date,count)|view!{<div class="daily-column" title=format!("{date}：{count} 筆")><strong>{*count}</strong><div class="daily-bar" style=format!("height:{}px",(*count as f64/maximum as f64*180.0))></div><span>{date[5..].to_owned()}</span></div>}).collect_view()}
        </div><figcaption>"日期 · UTC；只含明確發生時間的已提交事件"</figcaption></figure>
       }.into_any()}}
       <details><summary>"每日數據明細"</summary><table><thead><tr><th>"日期（UTC）"</th><th>"事件數"</th></tr></thead><tbody>{report.rows.into_iter().map(|(d,n)|view!{<tr><td>{d}</td><td>{n}</td></tr>}).collect_view()}</tbody></table></details>
      </section>
    }
}
