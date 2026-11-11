use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use chat_server::{logout, signin};
use sha2::{Digest, Sha256};
use tower::ServiceExt;

fn hash(t: &str) -> String {
    Sha256::digest(t.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test]
async fn day26_logout_revokes_only_one_session() {
    let db = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    chat_persistence::migrate(&db).await.unwrap();
    sqlx::query("INSERT INTO mbr_user (id,user_acnt,user_pd,user_email,user_name,user_status,user_sexy,user_birthday,is_init_pd,created_at,updated_at) VALUES('m1','user','hash','u@test.example','User','active',NULL,NULL,FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
        .execute(&db).await.unwrap();
    let a = "a".repeat(64);
    let b = "b".repeat(64);
    for (id, raw) in [("s1", &a), ("s2", &b)] {
        sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES(?1,'m1',?2,unixepoch()+3600)")
            .bind(id).bind(hash(raw)).execute(&db).await.unwrap();
    }
    let app = axum::Router::new()
        .merge(signin::routes(db.clone()))
        .merge(logout::routes(db.clone()));
    let req = |raw: &str| {
        Request::get("/account")
            .header(header::COOKIE, format!("chat_session={raw}"))
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        app.clone().oneshot(req(&a)).await.unwrap().status(),
        StatusCode::OK
    );
    let csrf = hash(&format!("logout:{a}"));
    let bad = app
        .clone()
        .oneshot(
            Request::post("/logout")
                .header(header::ORIGIN, "http://127.0.0.1:3000")
                .header(header::COOKIE, format!("chat_session={a}"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("csrf=wrong"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::FORBIDDEN);
    let ok = app
        .clone()
        .oneshot(
            Request::post("/logout")
                .header(header::ORIGIN, "http://127.0.0.1:3000")
                .header(header::COOKIE, format!("chat_session={a}"))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!("csrf={csrf}")))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::SEE_OTHER);
    assert!(
        ok.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        app.clone().oneshot(req(&a)).await.unwrap().status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(app.oneshot(req(&b)).await.unwrap().status(), StatusCode::OK);
}
