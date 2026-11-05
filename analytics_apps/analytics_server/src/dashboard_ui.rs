use analytics_ui::navigation::NavigationPage;
use leptos::attr::Allow;
use leptos::prelude::*;

use analytics_persistence::EventSummary;

use crate::dashboard_summary::summary_count_label;
use crate::dashboard_url::{encode_query_component, export_csv_url, source_switch_url};
use crate::management_shell::ManagementShell;

/// 只格式化既有篩選文案，不影響來源與事件類型的授權。
fn filter_description(event_type: &str) -> String {
    if event_type.is_empty() {
        "全部事件類型".to_owned()
    } else {
        format!("精確事件類型：{event_type}")
    }
}

#[component]
fn DashboardView(
    source_label: &'static str,
    total: i64,
    source: String,
    event_type: String,
    rows: Vec<(String, i64)>,
    logout_csrf: String,
    allow_chat: bool,
    allow_pet: bool,
    pages: Vec<NavigationPage>,
    can_export: bool,
) -> impl IntoView {
    let chat_current = (source_label == "Chat").then_some("page");
    let pet_current = (source_label == "Pet").then_some("page");
    let chat_source_url = source_switch_url("web_chat", &event_type);
    let pet_source_url = source_switch_url("desktop_pet", &event_type);
    let clear_url = format!("/dashboard?source={source}");
    let export_url = export_csv_url(&source, &event_type);
    let filter_description = filter_description(&event_type);

    let count_label = summary_count_label(source_label, &event_type, total);
    let filter_source = source.clone();

    view! {
        <html lang="zh-Hant">
            <head>
                <meta charset="utf-8"/>
                <title>"Analytics 管理後台"</title>
                <style>{include_str!("dashboard.css")}</style>
            </head>
            <body>
                <ManagementShell pages logout_csrf>
                    <nav aria-label="事件來源">
                        {allow_chat.then(|| view! {
                            <a href=chat_source_url aria-current=chat_current>"Chat 事件"</a>
                        })}
                        {allow_pet.then(|| view! {
                            <a href=pet_source_url aria-current=pet_current>"Pet 事件"</a>
                        })}
                    </nav>
                    {can_export.then(|| view! { <a href=export_url>"匯出目前篩選 CSV"</a> })}
                    <form method="get" action="/dashboard">
                        <input type="hidden" name="source" value=source/>
                        <label for="event-type">"事件類型（精確比對）"</label>
                        <input
                            id="event-type"
                            name="event_type"
                            value=event_type
                            maxlength="128"
                            placeholder="例如 chat.message.persisted"
                        />
                        <button type="submit">"篩選"</button>
                        <a href=clear_url>"清除篩選"</a>
                    </form>
                    <p>{filter_description}</p>
                    <p class="summary-count" aria-live="polite">{count_label}</p>
                    <table>
                        <caption>"已提交事件類型明細"</caption>
                        <thead><tr><th scope="col">"事件類型"</th><th scope="col">"已提交筆數"</th></tr></thead>
                        <tbody>
                            {rows.into_iter().map(|(kind, count)| {
                                let detail_url = format!(
                                    "/dashboard?source={}&event_type={}",
                                    filter_source,
                                    encode_query_component(&kind)
                                );
                                view! { <tr>
                                    <td>
                                        <a href=detail_url>{kind}</a>
                                    </td>
                                    <td>{count}</td>
                                </tr> }
                            }).collect_view()}
                        </tbody>
                    </table>
                    <p role="status">
                        {if total == 0 {
                            "沒有符合條件的已提交事件"
                        } else {
                            "查詢完成"
                        }}
                    </p>
                    </ManagementShell>
            </body>
        </html>
    }
}

pub fn render_dashboard_filtered(
    source_label: &'static str,
    total: i64,
    source: &str,
    event_type: &str,
) -> String {
    render_dashboard_with_rows(source_label, total, source, event_type, &[])
}

/// The real dashboard passes committed PG summary rows; older render callers remain valid.
pub fn render_dashboard_with_rows(
    source_label: &'static str,
    total: i64,
    source: &str,
    event_type: &str,
    rows: &[EventSummary],
) -> String {
    render_dashboard_with_rows_csrf(source_label, total, source, event_type, rows, "")
}

/// Real authenticated dashboard caller passes a Session-derived logout CSRF.
/// The old renderer remains for SSR snapshot tests, never as an auth decision.
pub fn render_dashboard_with_rows_csrf(
    source_label: &'static str,
    total: i64,
    source: &str,
    event_type: &str,
    rows: &[EventSummary],
    logout_csrf: &str,
) -> String {
    // 相容快照明示授權頁；正式路由傳入資料庫計算結果。
    let pages = vec![NavigationPage {
        category_id: "snapshot-member".into(),
        category_name: "會員管理".into(),
        page_id: "snapshot-password".into(),
        page_name: "修改密碼".into(),
        path: "/change-password".into(),
    }];
    render_dashboard_authorized(
        source_label,
        total,
        source,
        event_type,
        rows,
        logout_csrf,
        true,
        true,
        pages,
        true,
    )
}

/// 正式後台只輸出已授權來源的導覽連結；伺服器查詢仍獨立核權。
pub fn render_dashboard_with_rows_csrf_acl(
    source_label: &'static str,
    total: i64,
    source: &str,
    event_type: &str,
    rows: &[EventSummary],
    logout_csrf: &str,
    allow_chat: bool,
    allow_pet: bool,
) -> String {
    render_dashboard_authorized(
        source_label,
        total,
        source,
        event_type,
        rows,
        logout_csrf,
        allow_chat,
        allow_pet,
        Vec::new(),
        true,
    )
}

pub(crate) fn render_dashboard_authorized(
    source_label: &'static str,
    total: i64,
    source: &str,
    event_type: &str,
    rows: &[EventSummary],
    logout_csrf: &str,
    allow_chat: bool,
    allow_pet: bool,
    pages: Vec<NavigationPage>,
    can_export: bool,
) -> String {
    let rows: Vec<(String, i64)> = rows
        .iter()
        .map(|row| (row.event_type.clone(), row.total))
        .collect();
    format!(
        "<!doctype html>{}",
        view! {
            <DashboardView
                source_label
                total
                source=source.to_string()
                event_type=event_type.to_string()
                rows
                logout_csrf=logout_csrf.to_string()
                allow_chat=allow_chat
                allow_pet=allow_pet
                pages
                can_export
            />
        }
        .to_html()
    )
}

// 既有 Day28/29 測試與舊呼叫者仍可使用兩參數版本。
pub fn render_dashboard(source_label: &'static str, total: i64) -> String {
    let source = if source_label == "Pet" {
        "desktop_pet"
    } else {
        "web_chat"
    };
    render_dashboard_filtered(source_label, total, source, "")
}

#[component]
fn LoginView(username: String, message: String) -> impl IntoView {
    view! {
        <html lang="zh-Hant">
            <head>
                <meta charset="utf-8"/>
                <title>"Analytics 帳密登入"</title>
            </head>
            <body>
                <main>
                    <h1>"Analytics 帳密登入"</h1>
                    <p role="alert">{message}</p>
                    <form method="post" action="/login">
                        <div>
                            <label for="analytics-username">"帳號"</label>
                            <input id="analytics-username" name="username" value=username
                                autocomplete="username" required/>
                        </div>
                        <div>
                            <label for="analytics-password">"密碼"</label>
                            <input id="analytics-password" name="password"
                                type="password" autocomplete="current-password" required/>
                        </div>
                        <button type="submit">"登入"</button>
                    </form>
                </main>
            </body>
        </html>
    }
}

pub fn render_login() -> String {
    render_login_result("", "")
}

pub fn render_login_result(username: &str, message: &str) -> String {
    format!(
        "<!doctype html>{}",
        view! {<LoginView username=username.to_owned() message=message.to_owned()/>}.to_html()
    )
}

#[component]
fn ChangePasswordView(
    restricted: bool,
    csrf: String,
    logout_csrf: String,
    pages: Vec<NavigationPage>,
    can_update: bool,
) -> impl IntoView {
    let heading = if restricted {
        "首次登入須更換密碼與Email"
    } else {
        "修改密碼與Email"
    };
    view! {
        <html lang="zh-Hant">
            <head><meta charset="utf-8"/><title>"修改密碼"</title><style>{include_str!("dashboard.css")}</style></head>
            <body><ManagementShell pages logout_csrf>
                <h1>{heading}</h1>
                {if can_update { view! { <form method="post" action="/change-password">
                    <input type="hidden" name="csrf" value=csrf/>
                    <p>
                        <label for="change-current">"目前密碼"</label>
                        <input id="change-current" type="password" name="current_password"
                            autocomplete="current-password" required maxlength="128"/>
                    </p>
                    <p>
                        <label for="change-new">"新密碼"</label>
                        <input id="change-new" type="password" name="new_password"
                            autocomplete="new-password" required minlength="8" maxlength="128"/>
                    </p>
                    <p>
                        <label for="change-confirm">"確認新密碼"</label>
                        <input id="change-confirm" type="password" name="confirm_password"
                            autocomplete="new-password" required minlength="8" maxlength="128"/>
                    </p>
                    <p>
                        <label for="change-email">"唯一Email"</label>
                        <input id="change-email" type="email" name="email"
                            autocomplete="email" required maxlength="254"/>
                    </p>
                    <button type="submit">"儲存並重新登入"</button>
                </form> }.into_any() } else { view! { <p>"沒有修改密碼 action；請聯絡管理員處理授權。"</p> }.into_any() }}
                <p>"成功後所有裝置都須重新登入。Email不驗證；寄信重設尚未接通。"</p>
                </ManagementShell></body>
        </html>
    }
}

pub fn render_change_password(restricted: bool, csrf: String) -> String {
    render_change_password_with_logout(restricted, csrf, String::new())
}

pub fn render_change_password_with_logout(
    restricted: bool,
    csrf: String,
    logout_csrf: String,
) -> String {
    render_change_password_authorized(restricted, csrf, logout_csrf, Vec::new(), true)
}

pub(crate) fn render_change_password_authorized(
    restricted: bool,
    csrf: String,
    logout_csrf: String,
    pages: Vec<NavigationPage>,
    can_update: bool,
) -> String {
    format!(
        "<!doctype html>{}",
        view! {
            <ChangePasswordView restricted csrf logout_csrf pages can_update/>
        }
        .to_html()
    )
}

#[cfg(test)]
mod day43_source_switch_tests {
    use super::*;
    #[test]
    fn source_navigation_preserves_exact_utf8_filter_and_escapes_markup() {
        let html = render_dashboard_filtered("Chat", 1, "web_chat", "chat.中文 &");
        assert!(html.contains("source=desktop_pet&amp;event_type=chat.%E4%B8%AD%E6%96%87%20%26"));
        assert!(html.contains("source=web_chat&amp;event_type=chat.%E4%B8%AD%E6%96%87%20%26"));
        assert!(!html.contains("event_type=chat.中文 &"));
        let all = render_dashboard_filtered("Chat", 0, "web_chat", "");
        assert!(all.contains("/dashboard?source=desktop_pet"));
    }
}

#[cfg(test)]
mod day42_kind_navigation_tests {
    use super::*;
    #[test]
    fn kind_carow_links_to_real_exact_filter_and_escapes_html() {
        let html = render_dashboard_with_rows(
            "Chat",
            1,
            "web_chat",
            "",
            &[EventSummary {
                source_app: "web_chat".into(),
                event_type: "chat.中文 &".into(),
                total: 1,
            }],
        );
        assert!(html.contains("event_type=chat.%E4%B8%AD%E6%96%87%20%26"));
        assert!(html.contains("href="));
        assert!(html.contains("chat.中文 &amp;"));
        assert!(!html.contains("event_type=chat.中文 &"));
    }
}

#[cfg(test)]
mod day29_tests {
    use super::*;

    #[test]
    fn active_source_is_marked_for_accessibility() {
        let html = render_dashboard("Pet", 3);
        assert!(html.contains("desktop_pet"));
        assert!(html.contains("aria-current=\"page\""));
        assert!(html.contains("全部事件類型：3 筆已提交事件"));
    }
}

#[cfg(test)]
mod day30_tests {
    #[test]
    fn login_page_is_server_rendered_with_accessible_fields() {
        let html = super::render_login();
        assert_eq!(html.matches("<!doctype html>").count(), 1);
        assert!(html.contains("action=\"/login\""));
        assert!(html.contains("name=\"username\""));
        assert!(html.contains("name=\"password\""));
        assert!(html.contains("autocomplete=\"current-password\""));
    }
}

#[cfg(test)]
mod day31_tests {
    #[test]
    fn change_form_keeps_wire_names_and_escapes_attribute_values() {
        let html = super::render_change_password(true, "a\" onfocus=\"bad".into());
        assert_eq!(html.matches("<!doctype html>").count(), 1);
        for name in [
            "current_password",
            "new_password",
            "confirm_password",
            "email",
            "csrf",
        ] {
            assert!(html.contains(&format!("name=\"{name}\"")));
        }
        assert!(html.contains("&quot;"));
        assert!(!html.contains("value=\"a\" onfocus="));
        assert!(html.contains("首次登入須更換密碼與Email"));
        let ordinary = super::render_change_password(false, "test-csrf".into());
        assert!(ordinary.contains("修改密碼與Email"));
        assert!(!ordinary.contains("首次登入須更換密碼與Email"));
        assert!(!ordinary.contains("lesson-only-password"));
    }
}

#[cfg(test)]
mod day32_filter_tests {
    #[test]
    fn dashboard_has_accessible_exact_kind_filter_and_escaped_value() {
        let html = super::render_dashboard_filtered("Chat", 0, "web_chat", "chat.\" onfocus=\"bad");
        assert_eq!(html.matches("<!doctype html>").count(), 1);
        assert!(html.contains("method=\"get\""));
        assert!(html.contains("action=\"/dashboard\""));
        assert!(html.contains("name=\"source\" value=\"web_chat\""));
        assert!(html.contains("for=\"event-type\""));
        assert!(html.contains("id=\"event-type\""));
        assert!(html.contains("name=\"event_type\""));
        assert!(html.contains("&quot;"));
        assert!(!html.contains("value=\"chat.\" onfocus="));
        assert!(html.contains("沒有符合條件的已提交事件"));
        assert!(html.contains("/dashboard?source=web_chat"));
    }
}

#[cfg(test)]
mod day33_summary_caption_tests {
    #[test]
    fn count_label_distinguishes_full_and_exact_kind() {
        assert_eq!(
            super::summary_count_label("Chat", "", 12),
            "Chat 全部事件類型：12 筆已提交事件"
        );
        assert_eq!(
            super::summary_count_label("Chat", "chat.message.persisted", 3),
            "Chat 事件類型「chat.message.persisted」：3 筆已提交事件"
        );
        assert_eq!(
            super::summary_count_label("Pet", "pet.sleep", 0),
            "Pet 事件類型「pet.sleep」：0 筆已提交事件"
        );
    }

    #[test]
    fn real_leptos_ssr_escapes_filtered_type_in_count_label() {
        let html =
            super::render_dashboard_filtered("Chat", 3, "web_chat", "<img src=x onerror=alert(1)>");
        assert!(html.contains("3 筆已提交事件"));
        assert!(html.contains("class=\"summary-count\""));
        assert!(!html.contains("<img src=x onerror=alert(1)>"));
        assert!(html.contains("&lt;img"));
    }
}

#[cfg(test)]
mod day40_csv_filter_ui_tests {
    use super::*;
    #[test]
    fn dashboard_csv_link_encodes_exact_kind_and_escapes_markup() {
        assert_eq!(
            encode_query_component("a &中文"),
            "a%20%26%E4%B8%AD%E6%96%87"
        );
        let html = render_dashboard_filtered("Chat", 1, "web_chat", "x&y");
        assert!(html.contains("/api/v1/events/web_chat/export.csv?event_type=x%26y"));
        assert!(!html.contains("event_type=x&y"));
        let empty = render_dashboard_filtered("Chat", 0, "web_chat", "");
        assert!(empty.contains("/api/v1/events/web_chat/export.csv"));
    }
}

#[cfg(test)]
mod day41_event_table_tests {
    use super::*;
    #[test]
    fn committed_kind_rows_are_rendered_in_a_real_leptos_table_and_escaped() {
        let html = render_dashboard_with_rows(
            "Chat",
            3,
            "web_chat",
            "",
            &[
                EventSummary {
                    source_app: "web_chat".into(),
                    event_type: "chat.message.persisted".into(),
                    total: 2,
                },
                EventSummary {
                    source_app: "web_chat".into(),
                    event_type: "<script>bad</script>".into(),
                    total: 1,
                },
            ],
        );
        assert!(html.contains("<table"));
        assert!(html.contains("已提交事件類型明細"));
        assert!(html.contains("chat.message.persisted"));
        assert!(html.contains("&lt;script&gt;bad&lt;/script&gt;"));
        assert!(!html.contains("<script>bad</script>"));
        assert!(html.contains("全部事件類型：3 筆已提交事件"));
    }
}

#[cfg(test)]
mod day44_logout_csrf_ssr_tests {
    use super::*;
    #[test]
    fn both_leptos_forms_include_distinct_logout_csrf_without_leaking_raw_cookie() {
        let html = render_dashboard_with_rows_csrf("Chat", 0, "web_chat", "", &[], "logout-hex");
        let logout = html.split("action=\"/logout\"").nth(1).unwrap();
        assert!(logout.contains("name=\"csrf\" value=\"logout-hex\""));
        let change =
            render_change_password_with_logout(true, "change-hex".into(), "logout-hex".into());
        assert!(change.contains("name=\"csrf\" value=\"change-hex\""));
        let logout = change.split("action=\"/logout\"").nth(1).unwrap();
        assert!(logout.contains("name=\"csrf\" value=\"logout-hex\""));
    }
}

#[cfg(test)]
mod day48_source_navigation_acl_tests {
    use super::*;

    #[test]
    fn source_links_only_show_authorized_sources() {
        let only_pet = render_dashboard_with_rows_csrf_acl(
            "Pet",
            0,
            "desktop_pet",
            "",
            &[],
            "csrf",
            false,
            true,
        );
        assert!(only_pet.contains("Pet 事件"));
        assert!(!only_pet.contains("Chat 事件"));
        assert!(!only_pet.contains("source=web_chat"));
        let only_chat = render_dashboard_with_rows_csrf_acl(
            "Chat",
            0,
            "web_chat",
            "",
            &[],
            "csrf",
            true,
            false,
        );
        assert!(only_chat.contains("Chat 事件"));
        assert!(!only_chat.contains("Pet 事件"));
        assert!(!only_chat.contains("source=desktop_pet"));
    }
}

#[cfg(test)]
mod day50_grid_layout_tests {
    use super::*;
    #[test]
    fn real_dashboard_has_semantic_three_row_two_column_shell() {
        let html = render_dashboard_filtered("Chat", 1, "web_chat", "");
        for tag in [
            "<header class=\"analytics-header\"",
            "<aside class=\"analytics-sidebar\"",
            "<main class=\"analytics-content\"",
            "<footer class=\"analytics-footer\"",
        ] {
            assert!(html.contains(tag), "缺少 {tag}");
        }
        assert!(html.contains("grid-template-areas"));
        assert!(html.find("analytics-header").unwrap() < html.find("analytics-sidebar").unwrap());
        assert!(html.find("analytics-sidebar").unwrap() < html.find("analytics-content").unwrap());
        assert!(html.find("analytics-content").unwrap() < html.find("analytics-footer").unwrap());
        assert!(html.contains("action=\"/dashboard\""));
        assert!(html.contains("已提交事件類型明細"));
    }
}

#[cfg(test)]
mod component_acl_revision_tests {
    use super::*;
    #[test]
    fn ungranted_management_shortcuts_are_absent_before_serialization() {
        let html = render_dashboard_with_rows_csrf_acl(
            "Chat",
            0,
            "web_chat",
            "",
            &[],
            "csrf",
            false,
            false,
        );
        assert!(!html.contains("href=\"/accounts/new\""));
        assert!(!html.contains("href=\"/change-password\""));
        assert!(!html.contains("source=desktop_pet"));
        assert!(html.contains("analytics-shell"));
    }
    #[test]
    fn password_page_uses_the_same_semantic_shell() {
        let html =
            render_change_password_with_logout(true, "change-token".into(), "logout-token".into());
        for marker in [
            "<header",
            "<aside",
            "analytics-content",
            "<footer",
            "logout-token",
            "change-token",
        ] {
            assert!(html.contains(marker), "缺少 {marker}");
        }
        assert_eq!(html.matches("<html").count(), 1);
        assert_eq!(html.matches("action=\"/logout\"").count(), 1);
    }
    #[test]
    fn protected_rendering_never_patches_serialized_html() {
        for code in [
            include_str!("page_acl.rs"),
            include_str!("accounts.rs"),
            include_str!("password_login.rs"),
        ] {
            assert!(!code.contains("replace_range("));
            assert!(!code.contains("page_acl::decorate("));
        }
    }
}

#[cfg(test)]
mod authorized_view_tests {
    use super::*;
    #[test]
    fn grants_and_action_flags_are_applied_by_components() {
        let pages = vec![NavigationPage {
            category_id: "members".into(),
            category_name: "<會員>".into(),
            page_id: "password".into(),
            page_name: "自身密碼".into(),
            path: "/change-password".into(),
        }];
        let html = render_dashboard_authorized(
            "Chat",
            0,
            "web_chat",
            "",
            &[],
            "logout",
            false,
            false,
            pages.clone(),
            false,
        );
        assert!(html.contains("href=\"/change-password\""));
        assert!(html.contains("&lt;會員&gt;"));
        assert!(!html.contains("href=\"/accounts/new\""));
        assert!(!html.contains("匯出目前篩選 CSV"));
        assert!(!html.contains("data-acl-action"));
        let denied = render_change_password_authorized(
            false,
            "change".into(),
            "logout".into(),
            pages,
            false,
        );
        assert!(!denied.contains("action=\"/change-password\""));
        assert!(denied.contains("沒有修改密碼 action"));
        assert!(denied.contains("action=\"/logout\""));
    }
}
