#[path = "support/http.rs"]
mod http;
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header},
    routing::any,
};

async fn echo(request: Request<Body>) -> String {
    let method = request.method().to_string();
    let path = request.uri().to_string();
    let origin = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let content_type = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let cookie = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let body = axum::body::to_bytes(request.into_body(), 4096)
        .await
        .unwrap();
    format!(
        "{method}|{path}|{origin}|{content_type}|{cookie}|{}",
        String::from_utf8(body.to_vec()).unwrap()
    )
}

#[tokio::test]
async fn request_setup_preserves_get_and_form_wire_contracts() {
    let app = Router::new().route("/echo", any(echo));
    let get = http::get(app.clone(), "/echo?source=web_chat", None).await;
    assert_eq!(get.status(), StatusCode::OK);
    assert_eq!(http::body_text(get).await, "GET|/echo?source=web_chat||||");
    let form = "username=ADMIN&password=a%26b";
    let post = http::post_form(
        app.clone(),
        "/echo",
        form.into(),
        Some("analytics_session=fixture"),
    )
    .await;
    assert_eq!(
        http::body_text(post).await,
        "POST|/echo|http://127.0.0.1:3100|application/x-www-form-urlencoded|analytics_session=fixture|username=ADMIN&password=a%26b"
    );
    let with_cookie = http::send(
        app,
        "/echo",
        None,
        Some("theme=dark; analytics_session=fixture"),
    )
    .await;
    assert_eq!(
        http::body_text(with_cookie).await,
        "GET|/echo|||theme=dark; analytics_session=fixture|"
    );
}

#[tokio::test]
async fn response_helpers_keep_cookie_and_body_contracts() {
    let response = axum::response::Response::builder()
        .header(
            header::SET_COOKIE,
            "analytics_session=abc; HttpOnly; SameSite=Strict",
        )
        .body(Body::from("登入完成"))
        .unwrap();
    assert_eq!(http::response_cookie(&response), "analytics_session=abc");
    assert_eq!(http::body_text(response).await, "登入完成");
}
