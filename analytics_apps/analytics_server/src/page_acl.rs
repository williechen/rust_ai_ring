use crate::{
    IngestState,
    password_login::{digest, session},
};
use analytics_persistence::navigation::TreeRow;
use analytics_ui::navigation::{Navigation, NavigationPage};
use axum::{
    extract::{MatchedPath, Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use leptos::prelude::*;

// Requires a valid three-level tree and separate effective grants on both page and action.
// User deny on category/page/action wins; allowing a category never grants descendant CRUD.
pub(crate) use analytics_persistence::page_permissions::PERMISSION_SQL;

#[derive(Clone, Copy)]
pub(crate) enum Crud {
    Create,
    Read,
    Update,
    Delete,
}

impl Crud {
    fn key(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Read => "read",
            Self::Update => "update",
            Self::Delete => "delete",
        }
    }
}

// The server maps actual protocol operations; the caller cannot submit its own permission name.
fn operation(route: &str, method: &Method) -> Option<Crud> {
    match (route, method) {
        ("/accounts", &Method::POST) => Some(Crud::Create),
        ("/accounts/{id}/sources", &Method::POST) => Some(Crud::Update),
        ("/accounts/{id}/disable", &Method::POST) => Some(Crud::Delete),
        ("/accounts", &Method::DELETE) => Some(Crud::Delete),
        ("/change-password", &Method::POST) => Some(Crud::Update),
        ("/settings/session", &Method::POST) => Some(Crud::Update),
        (_, &Method::GET | &Method::HEAD) => Some(Crud::Read),
        _ => None,
    }
}

pub(crate) async fn require(
    State(state): State<IngestState>,
    request: Request,
    next: Next,
) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str())
        .unwrap_or(request.uri().path());
    let page = match route {
        "/" | "/dashboard" => "/dashboard",
        "/accounts" | "/accounts/new" | "/accounts/{id}/disable" | "/accounts/{id}/sources" => {
            "/accounts/new"
        }
        "/change-password" => "/change-password",
        "/settings/session" => "/settings/session",
        "/api/v1/events/{source}/summary"
        | "/api/v1/events/{source}/summary/{event_type}"
        | "/api/v1/events/{source}/export.csv" => route,
        _ => return next.run(request).await,
    };
    let Some(crud) = operation(route, request.method()) else {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    };
    let (restricted, raw, _seconds) = match session(&state, request.headers()).await {
        Ok(v) => v,
        Err(StatusCode::UNAUTHORIZED) if page == "/dashboard" => {
            return Redirect::to("/login").into_response();
        }
        Err(e) => return e.into_response(),
    };
    if restricted && page != "/change-password" {
        return if page == "/dashboard" {
            Redirect::to("/change-password").into_response()
        } else {
            StatusCode::FORBIDDEN.into_response()
        };
    }
    let permit = if request.method() == Method::GET
        && matches!(page, "/change-password" | "/accounts/new")
    {
        page_allowed(&state, &raw, page).await
    } else {
        allowed(&state, &raw, page, crud).await
    };
    match permit {
        Ok(true) => next.run(request).await,
        Ok(false) => (StatusCode::FORBIDDEN, "沒有此頁面或 CRUD action 的權限。").into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) async fn allowed(
    state: &IngestState,
    raw: &str,
    path: &str,
    operation: Crud,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(PERMISSION_SQL)
        .bind(digest(raw))
        .bind(path)
        .bind(operation.key())
        .fetch_one(&state.pool)
        .await
}

#[cfg(all(test, feature = "postgres-integration"))]
mod hierarchy_tests {
    use super::*;
    use crate::{IngestTokens, bootstrap};
    use std::sync::Arc;

    #[sqlx::test(migrations = "../migrations")]
    async fn three_levels_and_each_crud_grant_are_checked_independently(pool: sqlx::PgPool) {
        let id = bootstrap::initialize(&pool, "hierarchy-password".into())
            .await
            .unwrap();
        sqlx::query("UPDATE uac_user SET is_init_pd=FALSE WHERE id=$1")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        let credential: String = sqlx::query_scalar("SELECT user_pd FROM uac_user WHERE id=$1")
            .bind(&id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let raw = "f".repeat(64);
        sqlx::query("INSERT INTO uac_login_session(token_hash,user_id,credential_fingerprint,restricted,expires_at) VALUES($1,$2,$3,FALSE,floor(extract(epoch from now()))::bigint+1800)")
            .bind(digest(&raw)).bind(&id).bind(digest(&credential)).execute(&pool).await.unwrap();
        let state = IngestState {
            pool: pool.clone(),
            tokens: Arc::new(IngestTokens {
                chat: "fixture-chat".into(),
                pet: "fixture-pet".into(),
            }),
        };
        let roots:Vec<String>=sqlx::query_scalar("SELECT acl_name FROM uac_acl WHERE acl_category='category' AND acl_parent_id IS NULL ORDER BY acl_name").fetch_all(&pool).await.unwrap();
        assert_eq!(roots.len(), 3);
        for name in ["會員管理", "資訊分析", "認證管理"] {
            assert!(roots.iter().any(|n| n == name));
        }
        let malformed:i64=sqlx::query_scalar("SELECT count(*) FROM uac_acl a LEFT JOIN uac_acl p ON p.id=a.acl_parent_id LEFT JOIN uac_acl c ON c.id=p.acl_parent_id WHERE a.acl_category='action' AND (p.acl_category IS DISTINCT FROM 'page' OR c.acl_category IS DISTINCT FROM 'category' OR c.acl_parent_id IS NOT NULL)").fetch_one(&pool).await.unwrap();
        assert_eq!(malformed, 0);
        for op in [Crud::Create, Crud::Read, Crud::Update, Crud::Delete] {
            assert!(allowed(&state, &raw, "/accounts/new", op).await.unwrap());
        }
        sqlx::query("DELETE FROM uac_role_acl WHERE role_id='analytics-admin' AND acl_id='analytics-accounts-delete'").execute(&pool).await.unwrap();
        assert!(
            !allowed(&state, &raw, "/accounts/new", Crud::Delete)
                .await
                .unwrap()
        );
        assert!(
            allowed(&state, &raw, "/accounts/new", Crud::Read)
                .await
                .unwrap()
        );
        sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-accounts-delete','allow',now())").bind(&id).execute(&pool).await.unwrap();
        assert!(
            allowed(&state, &raw, "/accounts/new", Crud::Delete)
                .await
                .unwrap()
        );
        sqlx::query("UPDATE uac_user_acl SET acl_filter='deny' WHERE user_id=$1 AND acl_id='analytics-accounts-delete'").bind(&id).execute(&pool).await.unwrap();
        assert!(
            !allowed(&state, &raw, "/accounts/new", Crud::Delete)
                .await
                .unwrap()
        );
        assert!(
            allowed(&state, &raw, "/accounts/new", Crud::Create)
                .await
                .unwrap()
        );
        // Parent deny closes the subtree, leaving the other category usable.
        sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) VALUES($1,'analytics-members','deny',now())").bind(&id).execute(&pool).await.unwrap();
        assert!(
            !allowed(&state, &raw, "/accounts/new", Crud::Read)
                .await
                .unwrap()
        );
        assert!(
            allowed(&state, &raw, "/dashboard", Crud::Read)
                .await
                .unwrap()
        );
        sqlx::query("DELETE FROM uac_user_acl WHERE user_id=$1 AND acl_id='analytics-members'")
            .bind(&id)
            .execute(&pool)
            .await
            .unwrap();
        // Invalid/missing parent, inactive root, and duplicate action fail closed.
        sqlx::query(
            "UPDATE uac_acl SET acl_parent_id='analytics-accounts' WHERE id='analytics-accounts'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(
            !allowed(&state, &raw, "/accounts/new", Crud::Read)
                .await
                .unwrap()
        );
        sqlx::query(
            "UPDATE uac_acl SET acl_parent_id='analytics-members' WHERE id='analytics-accounts'",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE uac_acl SET acl_status='inactive' WHERE id='analytics-members'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            !allowed(&state, &raw, "/change-password", Crud::Update)
                .await
                .unwrap()
        );
        sqlx::query("INSERT INTO uac_acl(id,acl_name,acl_path,acl_category,acl_parent_id,acl_status,created_at,updated_at) VALUES('duplicate-read','read','read','action','analytics-accounts','active',now(),now())").execute(&pool).await.unwrap();
        assert!(
            !allowed(&state, &raw, "/accounts/new", Crud::Read)
                .await
                .unwrap()
        );
    }

    #[test]
    fn wire_operations_do_not_trust_a_client_supplied_action() {
        assert!(matches!(
            operation("/accounts", &Method::POST),
            Some(Crud::Create)
        ));
        assert!(matches!(
            operation("/change-password", &Method::POST),
            Some(Crud::Update)
        ));
        assert!(matches!(
            operation("/dashboard", &Method::GET),
            Some(Crud::Read)
        ));
        assert!(matches!(
            operation("/accounts", &Method::DELETE),
            Some(Crud::Delete)
        ));
        assert!(operation("/accounts", &Method::PATCH).is_none());
    }
}

async fn rows(state: &IngestState, raw: &str) -> Result<Vec<TreeRow>, sqlx::Error> {
    analytics_persistence::navigation::rows(&state.pool, &digest(raw)).await
}

async fn page_allowed(state: &IngestState, raw: &str, path: &str) -> Result<bool, sqlx::Error> {
    Ok(rows(state, raw).await?.iter().any(|r| r.4 == path))
}

pub(crate) async fn tree(state: &IngestState, raw: &str) -> Result<String, sqlx::Error> {
    Ok(render_navigation(rows(state, raw).await?))
}

fn navigation_pages(items: Vec<TreeRow>) -> Vec<NavigationPage> {
    items
        .into_iter()
        .map(
            |(category_id, category_name, page_id, page_name, path, _, _)| NavigationPage {
                category_id,
                category_name,
                page_id,
                page_name,
                path,
            },
        )
        .collect()
}

fn render_navigation(items: Vec<TreeRow>) -> String {
    let pages = navigation_pages(items);
    view! { <Navigation pages=pages /> }.to_html()
}

pub(crate) async fn navigation(
    State(state): State<IngestState>,
    headers: axum::http::HeaderMap,
) -> Result<Response, StatusCode> {
    let (_, raw, seconds) = session(&state, &headers).await?;
    Ok(crate::password_login::renewed(
        axum::response::Html(
            tree(&state, &raw)
                .await
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?,
        ),
        &raw,
        seconds,
    ))
}

/// 在序列化前取得有效頁面；查詢失敗不輸出業務頁面。
pub(crate) async fn pages(
    state: &IngestState,
    raw: &str,
) -> Result<Vec<NavigationPage>, StatusCode> {
    rows(state, raw)
        .await
        .map(navigation_pages)
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
}

#[cfg(test)]
mod day49_navigation_tests {
    use super::*;
    #[test]
    fn normal_navigation_hides_actions_but_keeps_page_links_and_escaping() {
        let page = |action: &str| {
            (
                "category".into(),
                "<分類>".into(),
                "page".into(),
                "<頁面>".into(),
                "/accounts/new".into(),
                Some(action.into()),
                Some("刪除".into()),
            )
        };
        let html = render_navigation(vec![page("read"), page("delete")]);
        assert!(!html.contains("data-acl-action"));
        assert!(!html.contains("刪除"));
        assert_eq!(html.matches("data-acl-category=").count(), 1);
        assert_eq!(html.matches("data-acl-page=").count(), 1);
        assert!(html.contains("href=\"/accounts/new\""));
        assert!(html.contains("&lt;分類&gt;"));
        assert!(html.contains("&lt;頁面&gt;"));
    }
    #[test]
    fn empty_grants_never_create_a_category_or_page() {
        let html = render_navigation(Vec::new());
        assert!(!html.contains("data-acl-category"));
        assert!(!html.contains("data-acl-page"));
    }
}
