use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chat_server::{logout, signin, signup};
use shared::auth::token_digest;
use tower::ServiceExt;

#[tokio::test]
async fn ambiguous_cookie_cannot_renew_revoke_or_register_then_valid_logout_works() {
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    chat_persistence::migrate(&db).await.unwrap();
    sqlx::query("INSERT INTO mbr_user(id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at) VALUES('m61','member61','hash','member61@example.test','會員','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
        .execute(&db).await.unwrap();
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    for (id, raw) in [("s61a", &a), ("s61b", &b)] {
        sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES(?1,'m61',?2,unixepoch()+3600)")
            .bind(id).bind(token_digest(raw)).execute(&db).await.unwrap();
    }
    let app = axum::Router::new()
        .merge(signin::routes(db.clone()))
        .merge(logout::routes(db.clone()))
        .merge(signup::routes(db.clone()));
    let snapshot = || async {
        sqlx::query_as::<_, (String, i64)>("SELECT id,expires_at FROM mbr_session ORDER BY id")
            .fetch_all(&db)
            .await
            .unwrap()
    };
    let before = snapshot().await;
    let csrf = token_digest(&format!("logout:{a}"));
    let invalid = [
        vec![format!("chat_session={a}; chat_session={a}")],
        vec![format!("chat_session={a}; chat_session={b}")],
        vec![format!("chat_session={a}"), format!("chat_session={b}")],
        vec![format!("chat_session={b}"), format!("chat_session={a}")],
        vec![format!("chat_session={a}; chat_session=bad")],
        vec![format!("chat_session={a}; chat_session")],
        vec![format!("chat_session={a}; chat_session ={b}")],
    ];
    for cookies in invalid {
        let mut request = Request::get("/api/session");
        for cookie in &cookies {
            request = request.header(header::COOKIE, cookie.as_str());
        }
        assert_eq!(
            app.clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let mut request = Request::post("/logout")
            .header(header::ORIGIN, "http://127.0.0.1:3000")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
        for cookie in &cookies {
            request = request.header(header::COOKIE, cookie.as_str());
        }
        assert_eq!(
            app.clone()
                .oneshot(request.body(Body::from(format!("csrf={csrf}"))).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(snapshot().await, before);
    }

    let signup = Request::post("/signup")
        .header(header::ORIGIN, "http://127.0.0.1:3000")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(
            header::COOKIE,
            format!("chat_session={a}; chat_session={b}"),
        )
        .body(Body::from(
            "username=new61&email=new61%40example.test&display_name=New&password=fixture-password",
        ))
        .unwrap();
    assert_eq!(
        app.clone().oneshot(signup).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    let members: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mbr_user")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(members, 1);
    assert_eq!(snapshot().await, before);

    let valid = Request::get("/api/session")
        .header(header::COOKIE, "other=1")
        .header(header::COOKIE, format!("chat_session={a}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        app.clone().oneshot(valid).await.unwrap().status(),
        StatusCode::NO_CONTENT
    );
    let logout = |value: &str| {
        Request::post("/logout")
            .header(header::ORIGIN, "http://127.0.0.1:3000")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::COOKIE, format!("other=1; chat_session={a}"))
            .body(Body::from(format!("csrf={value}")))
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(logout("wrong")).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
    let response = app.clone().oneshot(logout(&csrf)).await.unwrap();
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(
        response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    for (raw, expected) in [(&a, StatusCode::UNAUTHORIZED), (&b, StatusCode::NO_CONTENT)] {
        let req = Request::get("/api/session")
            .header(header::COOKIE, format!("chat_session={raw}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.clone().oneshot(req).await.unwrap().status(), expected);
    }
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM mbr_session ORDER BY id")
        .fetch_all(&db)
        .await
        .unwrap();
    assert_eq!(ids, vec!["s61b"]);
}
