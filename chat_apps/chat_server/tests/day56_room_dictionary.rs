use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
    routing::get,
};
use chat_server::{http, member_self, state::AppState};
use serde_json::{Value, json};
use sqlx::{SqlitePool, migrate::Migrator, sqlite::SqlitePoolOptions};
use tower::ServiceExt;

async fn setup(pool: &SqlitePool) -> Router {
    let state = AppState::new(
        leptos_config::LeptosOptions::builder()
            .output_name("fixture")
            .build(),
        Arc::new(chat_persistence::SqliteRoomRepository::new(pool.clone())),
        Arc::new(chat_persistence::SqliteMessageRepository::new(pool.clone())),
    );
    Router::new()
        .route("/api/rooms", get(http::list_rooms))
        .route("/api/rooms/{id}", get(http::get_room))
        .with_state(state)
        .merge(
            Router::new()
                .route(
                    "/api/rooms",
                    axum::routing::post(member_self::create_room_api),
                )
                .with_state(pool.clone()),
        )
        .merge(chat_server::signup::routes(pool.clone()))
        .merge(chat_server::signin::routes(pool.clone()))
}

async fn get_json(app: &Router, uri: &str) -> Value {
    let response = app
        .clone()
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    serde_json::from_slice(&to_bytes(response.into_body(), 32768).await.unwrap()).unwrap()
}

async fn post_form(app: &Router, uri: &str, form: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::post(uri)
                .header("origin", "http://127.0.0.1:3000")
                .header("content-type", "application/x-www-form-urlencoded")
                .body(Body::from(form.to_owned()))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn registered_member_creates_room_and_public_list_resolves_dictionary(pool: SqlitePool) {
    let app = setup(&pool).await;
    let response = post_form(&app,"/signup","username=member56&email=member56%40example.test&display_name=Day56&password=fixture-password").await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let response = post_form(
        &app,
        "/login",
        "username=member56&password=fixture-password",
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let cookie = response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    let raw = cookie.strip_prefix("chat_session=").unwrap();
    let csrf = shared::auth::token_digest(&format!("create-room:{raw}"));
    let create = |cookie: Option<&str>, token: Option<&str>| {
        let mut request = Request::post("/api/rooms")
            .header("content-type", "application/json")
            .header("origin", "http://127.0.0.1:3000");
        if let Some(cookie) = cookie {
            request = request.header("cookie", cookie);
        }
        if let Some(token) = token {
            request = request.header("x-csrf-token", token);
        }
        request
            .body(Body::from(json!({"name":"Day56 房間"}).to_string()))
            .unwrap()
    };
    assert_eq!(
        app.clone()
            .oneshot(create(None, None))
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.clone()
            .oneshot(create(Some(cookie), None))
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    let response = app
        .clone()
        .oneshot(create(Some(cookie), Some(&csrf)))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let created: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    let id = created["id"].as_str().unwrap();
    assert_eq!(created, json!({"id":id}));
    assert_eq!(
        get_json(&app, &format!("/api/rooms/{id}")).await,
        json!({"id":id,"category_room_id":"default-category","name":"Day56 房間"})
    );
    assert_eq!(
        get_json(&app, "/api/rooms").await,
        json!([{"id":id,"category_room_id":"default-category","name":"Day56 房間"}])
    );
    let rows = get_json(&app, "/api/rooms?q=Day56&labels=true").await;
    assert_eq!(
        rows,
        json!([{"id":id,"category_room_id":"default-category","name":"Day56 房間","category_label":"一般聊天室"}])
    );
}

async fn insert_room(pool: &SqlitePool, id: &str, category: &str) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO rooms VALUES (?1,?2,'舊房間','舊描述','2020-01-01','2020-01-02')")
        .bind(id)
        .bind(category)
        .execute(pool)
        .await
        .map(|_| ())
}

#[sqlx::test(migrations = false)]
async fn upgrades_old_labels_and_rolls_back_unknown_values(pool: SqlitePool) {
    let all = sqlx::migrate!("../migrations");
    let old = Migrator::with_migrations(
        all.iter()
            .filter(|m| m.version <= 20261105060000)
            .cloned()
            .collect(),
    );
    old.run(&pool).await.unwrap();
    insert_room(&pool, "old-room", "一般聊天室").await.unwrap();
    insert_room(&pool, "unknown-room", "未登錄種類")
        .await
        .unwrap();
    sqlx::query("INSERT INTO messages(id,room_id,author_kind,content) VALUES('old-message','old-room','system','舊訊息')")
        .execute(&pool).await.unwrap();
    let checksums: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(all.run(&pool).await.is_err(), "未知種類須使 migration 回滾");
    let old_value: String =
        sqlx::query_scalar("SELECT category_room_id FROM rooms WHERE id='old-room'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old_value, "一般聊天室");
    let tables: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name='codemap'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tables, 0);
    sqlx::query("UPDATE rooms SET category_room_id='程式設計' WHERE id='unknown-room'")
        .execute(&pool)
        .await
        .unwrap();
    all.run(&pool).await.unwrap();
    all.run(&pool).await.unwrap();
    let new: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(&new[..checksums.len()], checksums);
    assert_eq!(new.len(), checksums.len() + 1);
    let row: (String, String, String, String, String) = sqlx::query_as(
        "SELECT category_room_id,name,desc,created_at,updated_at FROM rooms WHERE id='old-room'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            "default-category".into(),
            "舊房間".into(),
            "舊描述".into(),
            "2020-01-01".into(),
            "2020-01-02".into()
        )
    );
    let message: String = sqlx::query_scalar(
        "SELECT content FROM messages WHERE id='old-message' AND room_id='old-room'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(message, "舊訊息");
    let app = setup(&pool).await;
    let rows = get_json(&app, "/api/rooms?labels=true").await;
    assert_eq!(rows[0]["category_label"], "一般聊天室");
    assert_eq!(rows[1]["category_label"], "程式設計");
}

#[sqlx::test(migrations = "../migrations")]
async fn sqlite_rejects_wrong_namespace_and_preserves_referenced_keys(pool: SqlitePool) {
    assert!(
        insert_room(&pool, "unknown", "unregistered").await.is_err(),
        "未知 key 不得寫入"
    );
    sqlx::query("INSERT INTO codemap(category,code,value) VALUES('other','other-key','其他')")
        .execute(&pool)
        .await
        .unwrap();
    assert!(insert_room(&pool, "wrong", "other-key").await.is_err());
    assert!(
        sqlx::query(
            "INSERT INTO codemap(category,code,value) VALUES('room_category','中文','中文')"
        )
        .execute(&pool)
        .await
        .is_err()
    );
    insert_room(&pool, "valid", "default-category")
        .await
        .unwrap();
    for sql in [
        "UPDATE rooms SET category_room_id='other-key' WHERE id='valid'",
        "DELETE FROM codemap WHERE category='room_category' AND code='default-category'",
        "UPDATE codemap SET code='elsewhere' WHERE category='room_category' AND code='default-category'",
    ] {
        assert!(sqlx::query(sql).execute(&pool).await.is_err(), "{sql}");
    }
    sqlx::query("UPDATE codemap SET value='更新後類別' WHERE category='room_category' AND code='default-category'")
        .execute(&pool).await.unwrap();
    let app = setup(&pool).await;
    assert_eq!(
        get_json(&app, "/api/rooms?labels=true").await[0]["category_label"],
        "更新後類別"
    );
}