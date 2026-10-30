use crate::{auth_session::active_member, room_patch::canonical_room_id};
use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use sqlx::SqlitePool;

/// Keep the existing #[cfg(test)] mod day25_tests unchanged.
pub async fn require_member(
    State(pool): State<SqlitePool>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    active_member(&pool, request.headers()).await?;
    Ok(next.run(request).await)
}

/// Only browser room navigation redirects; WS and write APIs retain 401.
pub async fn require_room_member(
    State(pool): State<SqlitePool>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    match active_member(&pool, request.headers()).await {
        Ok(_) => {
            // 登入成功也不能讓不存在的房間進入 SSR／WebSocket 畫面。
            // 沿用正式 SQLite RoomRepository，而不是在 middleware 複製 SQL。
            use chat_application::repository::RoomRepository;
            let id = canonical_room_id(request.uri().path()).ok_or(StatusCode::NOT_FOUND)?;
            let repo = chat_persistence::SqliteRoomRepository::new(pool.clone());
            let room = repo
                .find_by_id(&id)
                .await
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            if !room.is_some_and(|room| room.id == id) {
                return Err(StatusCode::NOT_FOUND);
            }
            Ok(next.run(request).await)
        }
        Err(StatusCode::UNAUTHORIZED) => {
            let id = canonical_room_id(request.uri().path()).ok_or(StatusCode::UNAUTHORIZED)?;
            Ok(Redirect::to(&format!("/signin?next=/room/{id}")).into_response())
        }
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod day41_room_return_tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    #[tokio::test]
    async fn anonymous_room_link_redirects_but_invalid_room_id_is_rejected() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let app = Router::new()
            .route("/room/{room_id}", get(|| async { StatusCode::OK }))
            .route_layer(axum::middleware::from_fn_with_state(
                pool,
                require_room_member,
            ));
        let id = uuid::Uuid::new_v4();
        let response = app
            .clone()
            .oneshot(
                Request::get(format!("/room/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            response.headers()["location"],
            format!("/signin?next=/room/{id}")
        );
        let bad = app
            .oneshot(
                Request::get("/room/not-a-uuid")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    }
}

#[cfg(test)]
mod day25_tests {
    use super::*;
    use axum::{
        Router,
        body::Body,
        http::{Request, StatusCode},
        routing::get,
    };
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    #[tokio::test]
    async fn room_page_requires_active_sqlite_session() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261018073931_create_auth_user.sql"
        ))
        .execute(&db)
        .await
        .unwrap();
        sqlx::raw_sql(include_str!(
            "../../migrations/20261020152737_mbr_sessions.sql"
        ))
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO mbr_user (id,user_acnt,user_pd,user_email,user_name,user_status,user_sexy,user_birthday,is_init_pd,created_at,updated_at) VALUES ('m1','member','unused','m1@example.test','Member','active','', '2000-01-01',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
            .execute(&db).await.unwrap();
        let token = "a".repeat(64);
        let hash = Sha256::digest(token.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        sqlx::query("INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at) VALUES('s1','m1',?1,unixepoch()+60)")
            .bind(hash).execute(&db).await.unwrap();
        let app = Router::new()
            .route("/room/{room_id}", get(|| async { StatusCode::OK }))
            .route_layer(axum::middleware::from_fn_with_state(
                db.clone(),
                require_member,
            ));
        for (cookie, expected) in [
            (None, StatusCode::UNAUTHORIZED),
            (
                Some("chat_session=invalid".to_owned()),
                StatusCode::UNAUTHORIZED,
            ),
            (Some(format!("chat_session={token}")), StatusCode::OK),
        ] {
            let mut request = Request::get("/room/real-room");
            if let Some(value) = cookie {
                request = request.header("cookie", value);
            }
            let response = app
                .clone()
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
        sqlx::query("UPDATE mbr_session SET expires_at=unixepoch()-1 WHERE id='s1'")
            .execute(&db)
            .await
            .unwrap();
        let expired = app
            .oneshot(
                Request::get("/room/real-room")
                    .header("cookie", format!("chat_session={token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(expired.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn anonymous_create_is_rejected_while_public_list_is_available() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        chat_persistence::migrate(&db).await.unwrap();
        let state = crate::state::AppState::new(
            leptos_config::get_configuration(None)
                .unwrap()
                .leptos_options,
            std::sync::Arc::new(chat_persistence::SqliteRoomRepository::new(db.clone())),
            std::sync::Arc::new(chat_persistence::SqliteMessageRepository::new(db.clone())),
        );
        let room_methods = get(crate::http::list_rooms).merge(
            axum::routing::post(crate::http::create_room).route_layer(
                axum::middleware::from_fn_with_state(db.clone(), require_member),
            ),
        );
        let app = Router::new()
            .route("/api/rooms", room_methods)
            .with_state(state);

        let anonymous = app
            .clone()
            .oneshot(
                Request::post("/api/rooms")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"name":"anonymous"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM rooms")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);

        let public = app
            .oneshot(Request::get("/api/rooms").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(public.status(), StatusCode::OK);
        db.close().await;
    }
}

#[cfg(test)]
mod day44_existing_room_gate_tests {
    use super::*;
    use axum::{Router, body::Body, http::Request, routing::get};
    use tower::ServiceExt;

    #[tokio::test]
    async fn signed_in_room_navigation_rejects_missing_and_allows_saved_room() {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        chat_persistence::migrate(&db).await.unwrap();
        let room_id = uuid::Uuid::new_v4().to_string();
        let missing_id = uuid::Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at)
            VALUES(?1,'default-category','實際房間','',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
        )
        .bind(&room_id)
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO mbr_user
            (id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at)
            VALUES('day44-user','day44','hash','day44@example.invalid','Member','active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)")
            .execute(&db).await.unwrap();
        let token = "a".repeat(64);
        sqlx::query(
            "INSERT INTO mbr_session(id,mbr_user_id,token_hash,expires_at)
            VALUES('day44-session','day44-user',?1,unixepoch()+60)",
        )
        .bind(shared::auth::token_digest(&token))
        .execute(&db)
        .await
        .unwrap();
        let app = Router::new()
            .route("/room/{room_id}", get(|| async { StatusCode::OK }))
            .route_layer(axum::middleware::from_fn_with_state(
                db.clone(),
                require_room_member,
            ));
        for (id, expected) in [
            (&room_id, StatusCode::OK),
            (&missing_id, StatusCode::NOT_FOUND),
        ] {
            let response = app
                .clone()
                .oneshot(
                    Request::get(format!("/room/{id}"))
                        .header("cookie", format!("chat_session={token}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{id}");
        }
        let anonymous = app
            .clone()
            .oneshot(
                Request::get(format!("/room/{room_id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::SEE_OTHER);
        assert_eq!(
            anonymous.headers()["location"],
            format!("/signin?next=/room/{room_id}")
        );
        db.close().await;
    }
}
