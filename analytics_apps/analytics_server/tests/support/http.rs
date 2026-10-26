// Each integration binary intentionally uses a different subset of shared transport helpers.
#![allow(dead_code)]
use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, header},
    response::Response,
};
use tower::ServiceExt;

pub async fn send(app: Router, path: &str, form: Option<&str>, cookie: Option<&str>) -> Response {
    let mut request = if form.is_some() {
        Request::post(path)
            .header(header::ORIGIN, "http://127.0.0.1:3100")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
    } else {
        Request::get(path)
    };
    if let Some(cookie) = cookie {
        request = request.header(header::COOKIE, cookie);
    }
    app.oneshot(
        request
            .body(form.map_or_else(Body::empty, |s| Body::from(s.to_owned())))
            .unwrap(),
    )
    .await
    .unwrap()
}

pub async fn get(app: Router, path: &str, cookie: Option<&str>) -> Response {
    send(app, path, None, cookie).await
}
pub async fn post_form(app: Router, path: &str, form: String, cookie: Option<&str>) -> Response {
    send(app, path, Some(&form), cookie).await
}

pub fn response_cookie(response: &Response) -> String {
    response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

pub async fn body_text(response: Response) -> String {
    String::from_utf8(
        to_bytes(response.into_body(), 32768)
            .await
            .unwrap()
            .to_vec(),
    )
    .unwrap()
}
