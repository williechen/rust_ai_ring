#![cfg(feature = "postgres-integration")]

use analytics_server::{IngestState, IngestTokens, build_app};
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
};
use std::sync::Arc;
use tower::ServiceExt;

async fn post_event(app: &Router, source: &str, auth: &[&str]) -> StatusCode {
    let body = serde_json::json!({
        "schema_version": 1,
        "event_id": uuid::Uuid::new_v4(),
        "source_app": source,
        "event_type": "test.ingest.authorization"
    });
    let mut request = Request::post("/api/v1/events")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    for value in auth {
        request
            .headers_mut()
            .append(header::AUTHORIZATION, value.parse().unwrap());
    }
    app.clone().oneshot(request).await.unwrap().status()
}

#[sqlx::test(migrations = "../migrations")]
async fn ambiguous_machine_authorization_is_rejected_before_db_write(pool: sqlx::PgPool) {
    let app = build_app(IngestState {
        pool: pool.clone(),
        tokens: Arc::new(IngestTokens {
            chat: "fixture-chat".into(),
            pet: "fixture-pet".into(),
        }),
    });

    assert_eq!(
        post_event(&app, "web_chat", &["Bearer fixture-chat"]).await,
        StatusCode::CREATED
    );
    for auth in [
        vec!["Bearer fixture-chat", "Bearer fixture-pet"],
        vec!["Bearer fixture-chat", "Bearer fixture-chat"],
        vec!["Bearer fixture-pet", "Bearer fixture-chat"],
        vec!["Bearer fixture-pet"],
        vec![],
    ] {
        assert_eq!(
            post_event(&app, "web_chat", &auth).await,
            StatusCode::UNAUTHORIZED,
            "authorization headers: {auth:?}"
        );
    }
    assert_eq!(
        post_event(&app, "desktop_pet", &["Bearer fixture-pet"]).await,
        StatusCode::CREATED
    );
    assert_eq!(
        post_event(&app, "desktop_pet", &["Bearer fixture-chat"]).await,
        StatusCode::UNAUTHORIZED
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product_events")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "only the two valid single-header events persist");
}
