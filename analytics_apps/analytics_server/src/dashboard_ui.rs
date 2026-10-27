use leptos::prelude::*;

use analytics_persistence::EventSummary;

fn summary_count_label(source_label: &str, event_type: &str, total: i64) -> String {
    if event_type.is_empty() {
        format!("{source_label} 全部事件類型：{total} 筆已提交事件")
    } else {
        format!("{source_label} 事件類型「{event_type}」：{total} 筆已提交事件")
    }
}

fn encode_query_component(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            result.push(char::from(byte));
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

#[component]
fn DashboardView(
    source_label: &'static str,
    total: i64,
    source: String,
    event_type: String,
    rows: Vec<(String, i64)>,
) -> impl IntoView {
    let chat_current = (source_label == "Chat").then_some("page");
    let pet_current = (source_label == "Pet").then_some("page");
    let clear_url = format!("/dashboard?source={source}");
    let export_url = if event_type.is_empty() {
        format!("/api/v1/events/{source}/export.csv")
    } else {
        format!(
            "/api/v1/events/{source}/export.csv?event_type={}",
            encode_query_component(&event_type)
        )
    };
    let filter_description = if event_type.is_empty() {
        "全部事件類型".to_string()
    } else {
        format!("精確事件類型：{event_type}")
    };

    let count_label = summary_count_label(source_label, &event_type, total);

    view! {
        <html lang="zh-Hant">
            <head>
                <meta charset="utf-8"/>
                <title>"Analytics 管理後台"</title>
            </head>
            <body>
                <main>
                    <h1>"Analytics 管理後台"</h1>
                    <nav aria-label="事件來源">
                        <a href="/dashboard?source=web_chat" aria-current=chat_current>
                            "Chat 事件"
                        </a>
                        <a href="/dashboard?source=desktop_pet" aria-current=pet_current>
                            "Pet 事件"
                        </a>
                    </nav>
                    <a href="/change-password">"修改密碼與 Email"</a>
                    <a href=export_url>"匯出目前篩選 CSV"</a>
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
                            {rows.into_iter().map(|(kind, count)| view! {
                                <tr><td>{kind}</td><td>{count}</td></tr>
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
                    <p>"目前僅支援管理員；角色與來源 ACL 尚待接通。"</p>
                    <form method="post" action="/logout">
                        <button type="submit">"登出"</button>
                    </form>
                </main>
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
fn LoginView() -> impl IntoView {
    view! {
        <html lang="zh-Hant">
            <head>
                <meta charset="utf-8"/>
                <title>"Analytics 帳密登入"</title>
            </head>
            <body>
                <main>
                    <h1>"Analytics 帳密登入"</h1>
                    <form method="post" action="/login">
                        <div>
                            <label for="analytics-username">"帳號"</label>
                            <input id="analytics-username" name="username"
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
    format!("<!doctype html>{}", view! { <LoginView/> }.to_html())
}

#[component]
fn ChangePasswordView(restricted: bool, csrf: String) -> impl IntoView {
    let heading = if restricted {
        "首次登入須更換密碼與Email"
    } else {
        "修改密碼與Email"
    };
    view! {
        <html lang="zh-Hant">
            <head><meta charset="utf-8"/><title>"修改密碼"</title></head>
            <body><main>
                <h1>{heading}</h1>
                <form method="post" action="/change-password">
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
                </form>
                <p>"成功後所有裝置都須重新登入。Email不驗證；寄信重設尚未接通。"</p>
                <form method="post" action="/logout">
                    <button type="submit">"登出"</button>
                </form>
            </main></body>
        </html>
    }
}

pub fn render_change_password(restricted: bool, csrf: String) -> String {
    format!(
        "<!doctype html>{}",
        view! {
            <ChangePasswordView restricted csrf/>
        }
        .to_html()
    )
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
