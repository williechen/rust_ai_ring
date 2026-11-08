use leptos::prelude::*;
use std::collections::BTreeMap;

#[derive(Clone)]
pub struct NavigationPage {
    pub category_id: String,
    pub category_name: String,
    pub page_id: String,
    pub page_name: String,
    pub path: String,
}

/// 一般功能樹只接收已授權頁面；CRUD是否可執行仍由伺服器判定。
#[component]
pub fn Navigation(pages: Vec<NavigationPage>) -> impl IntoView {
    let mut groups: BTreeMap<String, (String, BTreeMap<String, NavigationPage>)> = BTreeMap::new();
    for mut page in pages {
        // 來源授權仍分別核對，一般導覽只呈現共用的分析入口。
        match page.path.as_str() {
            "/api/v1/events/web_chat"
            | "/api/v1/events/desktop_pet"
            | "/api/v1/events/{source}/summary"
            | "/api/v1/events/{source}/summary/{event_type}"
            | "/api/v1/events/{source}/export.csv" => continue,
            "/dashboard" => page.page_name = "來源分析".into(),
            _ => {}
        }
        groups
            .entry(page.category_id.clone())
            .or_insert_with(|| (page.category_name.clone(), BTreeMap::new()))
            .1
            .entry(page.page_id.clone())
            .or_insert(page);
    }
    view! {
        <nav aria-label="功能樹"><ul>
            {groups.into_iter().map(|(category_id, (name, pages))| view! {
                <li data-acl-category=category_id>{name}<ul>
                    {pages.into_iter().map(|(id, page)| {
                        let href = match page.path.as_str() {
                            "/dashboard" | "/accounts/new" | "/change-password" => Some(page.path.clone()),
                            _ => None,
                        };
                        view! {
                            <li data-acl-page=id>{if let Some(href) = href {
                                view! { <a href=href>{page.page_name}</a> }.into_any()
                            } else { page.page_name.into_any() }}</li>
                        }
                    }).collect_view()}
                </ul></li>
            }).collect_view()}
        </ul></nav>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_rows_share_one_page_and_unknown_routes_stay_labels() {
        Owner::new().with(|| {
            let page = NavigationPage {
                category_id: "c".into(),
                category_name: "分類".into(),
                page_id: "p".into(),
                page_name: "<尚未接通>".into(),
                path: "javascript:alert(1)".into(),
            };
            let html = view! { <Navigation pages=vec![page.clone(), page] /> }.to_html();
            assert_eq!(html.matches("data-acl-page=").count(), 1);
            assert!(html.contains("&lt;尚未接通&gt;"));
            assert!(!html.contains("href="));
            assert!(!html.contains("javascript:"));
        });
    }
    #[test]
    fn sources_share_one_dashboard_navigation_entry() {
        Owner::new().with(|| {
            let pages = [
                "/dashboard",
                "/api/v1/events/web_chat",
                "/api/v1/events/desktop_pet",
                "/api/v1/events/{source}/summary",
                "/api/v1/events/{source}/summary/{event_type}",
                "/api/v1/events/{source}/export.csv",
            ]
            .into_iter()
            .map(|path| NavigationPage {
                category_id: "c".into(),
                category_name: "資訊分析".into(),
                page_id: path.into(),
                page_name: path.into(),
                path: path.into(),
            })
            .collect();
            let html = view! { <Navigation pages=pages /> }.to_html();
            assert_eq!(html.matches("data-acl-page=").count(), 1);
            assert!(html.contains("href=\"/dashboard\">來源分析</a>"));
            assert!(!html.contains("/api/v1/events/"));
        });
    }
}
