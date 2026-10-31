use crate::{
    IngestState,
    password_login::{digest, session},
};
use axum::{
    extract::{MatchedPath, Request, State},
    http::{Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

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
        ("/accounts", &Method::DELETE) => Some(Crud::Delete),
        ("/change-password", &Method::POST) => Some(Crud::Update),
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
        "/accounts" | "/accounts/new" => "/accounts/new",
        "/change-password" => "/change-password",
        "/api/v1/events/{source}/summary"
        | "/api/v1/events/{source}/summary/{event_type}"
        | "/api/v1/events/{source}/export.csv" => route,
        _ => return next.run(request).await,
    };
    let Some(crud) = operation(route, request.method()) else {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    };
    let (restricted, raw) = match session(&state, request.headers()).await {
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

// Requires a valid three-level tree and separate effective grants on both page and action.
// User deny on category/page/action wins; allowing a category never grants descendant CRUD.
pub(crate) const PERMISSION_SQL: &str = r#"
WITH node AS (
 SELECT p.id AS page_id,c.id AS category_id,a.id AS action_id
 FROM uac_acl p JOIN uac_acl c ON c.id=p.acl_parent_id
 JOIN uac_acl a ON a.acl_parent_id=p.id
 WHERE p.acl_category='page' AND p.acl_path=$2 AND p.acl_status='active'
  AND c.acl_category='category' AND c.acl_parent_id IS NULL AND c.acl_status='active'
  AND a.acl_category='action' AND a.acl_path=$3 AND a.acl_status='active'
  AND (SELECT count(*) FROM uac_acl p2 WHERE p2.acl_category='page' AND p2.acl_path=$2)=1
  AND (SELECT count(*) FROM uac_acl a2 WHERE a2.acl_parent_id=p.id AND a2.acl_category='action' AND a2.acl_path=$3)=1
), person AS (
 SELECT s.user_id FROM uac_login_session s JOIN uac_user u ON u.id=s.user_id
 WHERE s.token_hash=$1 AND s.expires_at>floor(extract(epoch from now()))::bigint
  AND u.user_status='active'
)
SELECT EXISTS(SELECT 1 FROM node n CROSS JOIN person p
 WHERE NOT EXISTS(SELECT 1 FROM uac_user_acl ua WHERE ua.user_id=p.user_id
   AND ua.acl_id IN(n.category_id,n.page_id,n.action_id) AND ua.acl_filter='deny')
 AND NOT EXISTS(SELECT 1 FROM (VALUES(n.page_id),(n.action_id)) AS needed(id)
   WHERE NOT EXISTS(SELECT 1 FROM uac_user_role ur JOIN uac_role r ON r.id=ur.role_id
     JOIN uac_role_acl ra ON ra.role_id=r.id AND ra.acl_id=needed.id
     WHERE ur.user_id=p.user_id AND r.role_status='active')
    AND NOT EXISTS(SELECT 1 FROM uac_user_acl ua WHERE ua.user_id=p.user_id
      AND ua.acl_id=needed.id AND ua.acl_filter='allow')))
"#;

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

// A page is visible without any action grant. Category visibility is a projection of pages.
const PAGE_SQL: &str = r#"
WITH person AS (
 SELECT s.user_id FROM uac_login_session s JOIN uac_user u ON u.id=s.user_id
 WHERE s.token_hash=$1 AND s.expires_at>floor(extract(epoch from now()))::bigint AND u.user_status='active'
), granted AS (
 SELECT a.* FROM uac_acl a CROSS JOIN person p WHERE a.acl_status='active'
 AND NOT EXISTS(SELECT 1 FROM uac_user_acl x WHERE x.user_id=p.user_id AND x.acl_id=a.id AND x.acl_filter='deny')
 AND (EXISTS(SELECT 1 FROM uac_user_acl x WHERE x.user_id=p.user_id AND x.acl_id=a.id AND x.acl_filter='allow')
 OR EXISTS(SELECT 1 FROM uac_user_role ur JOIN uac_role  ON r.id=ur.role_id JOIN uac_role_acl ra ON ra.role_id=r.id
 WHERE ur.user_id=p.user_id AND r.role_status='active' AND ra.acl_id=a.id))
)
SELECT c.id,c.acl_name,p.id,p.acl_name,p.acl_path,a.id,a.acl_name
FROM granted p JOIN uac_acl c ON c.id=p.acl_parent_id AND c.acl_category='category'
 AND c.acl_parent_id IS NULL AND c.acl_status='active'
CROSS JOIN person who
LEFT JOIN granted a ON a.acl_category='action' AND a.acl_parent_id=p.id
 AND a.acl_path IN ('create','read','update','delete')
 AND (SELECT count(*) FROM uac_acl other WHERE other.acl_parent_id=p.id AND other.acl_category='action' AND other.acl_path=a.acl_path)=1
WHERE p.acl_category='page'
 AND (SELECT count(*) FROM uac_acl other WHERE other.acl_category='page' AND other.acl_path=p.acl_path)=1
 AND NOT EXISTS(SELECT 1 FROM uac_user_acl x WHERE x.user_id=who.user_id AND x.acl_id=c.id AND x.acl_filter='deny')
ORDER BY c.id,p.id,a.id
"#;

type TreeRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);

async fn rows(state: &IngestState, raw: &str) -> Result<Vec<TreeRow>, sqlx::Error> {
    sqlx::query_as(PAGE_SQL)
        .bind(digest(raw))
        .fetch_all(&state.pool)
        .await
}

async fn page_allowed(state: &IngestState, raw: &str, path: &str) -> Result<bool, sqlx::Error> {
    Ok(rows(state, raw).await?.iter().any(|r| r.4 == path))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub(crate) async fn tree(state: &IngestState, raw: &str) -> Result<String, sqlx::Error> {
    let mut html = String::from("<nav aria-label=\"功能樹\"><ul>");
    let mut category = String::new();
    let mut page = String::new();
    for (cid, cname, pid, pname, path, aid, aname) in rows(state, raw).await? {
        if cid != category {
            if !page.is_empty() {
                html.push_str("</ul></li>");
                page.clear();
            }
            if !category.is_empty() {
                html.push_str("</ul></li>");
            }
            html.push_str(&format!(
                "<li data-acl-category=\"{}\">{}<ul>",
                escape(&cid),
                escape(&cname)
            ));
            category = cid;
        }
        if pid != page {
            if !page.is_empty() {
                html.push_str("</ul></li>");
            }
            html.push_str(&format!("<li data-acl-page=\"{}\">", escape(&pid)));
            // API definitions remain visible labels; do not manufacture unimplemented UI routes.
            let href = match path.as_str() {
                "/api/v1/events/web_chat" => "/dashboard?source=web_chat",
                "/api/v1/events/desktop_pet" => "/dashboard?source=desktop_pet",
                other => other,
            };
            if matches!(
                path.as_str(),
                "/dashboard"
                    | "/accounts/new"
                    | "/change-password"
                    | "/api/v1/events/web_chat"
                    | "/api/v1/events/desktop_pet"
            ) {
                html.push_str(&format!(
                    "<a href=\"{}\">{}</a>",
                    escape(href),
                    escape(&pname)
                ));
            } else {
                html.push_str(&escape(&pname));
            }
            html.push_str("<ul>");
            page = pid;
        }
        if let Some(id) = aid {
            html.push_str(&format!(
                "<li data-acl-action=\"{}\">{}</li>",
                escape(&id),
                escape(&aname.unwrap_or_default())
            ));
        }
    }
    if !page.is_empty() {
        html.push_str("</ul></li>");
    }
    if !category.is_empty() {
        html.push_str("</ul></li>");
    }
    html.push_str("</ul></nav>");
    Ok(html)
}

pub(crate) async fn navigation(
    State(state): State<IngestState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::response::Html<String>, StatusCode> {
    let (_, raw) = session(&state, &headers).await?;
    Ok(axum::response::Html(
        tree(&state, &raw)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?,
    ))
}

// Shared management shell on actual HTML callers, not just a standalone projection test.
pub(crate) async fn decorate(
    state: &IngestState,
    raw: &str,
    mut html: String,
) -> Result<String, StatusCode> {
    let nav = tree(state, raw)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    // Remove legacy unconditional shortcuts; the grant-derived tree owns management links.
    for literal in [
        "<a href=\"/change-password\">修改密碼與 Email</a>",
        "<a href=\"/accounts/new\">帳號管理：新增唯讀帳號</a>",
    ] {
        html = html.replace(literal, "");
    }
    if let Some(start) = html.find("<nav aria-label=\"事件來源\"") {
        if let Some(end) = html[start..].find("</nav>") {
            html.replace_range(start..start + end + 6, "");
        }
    }
    if !allowed(state, raw, "/api/v1/events/{source}/export.csv", Crud::Read)
        .await
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?
    {
        if let Some(label) = html.find("匯出目前篩選 CSV") {
            if let Some(start) = html[..label].rfind("<a ") {
                if let Some(end) = html[label..].find("</a>") {
                    html.replace_range(start..label + end + 4, "");
                }
            }
        }
    }
    Ok(html.replacen("<main>", &format!("<main>{nav}"), 1))
}
