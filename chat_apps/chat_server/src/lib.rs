pub mod auth_session;
pub mod http;
pub mod hub;
pub mod logout;
pub mod member_self;
pub mod room_auth;
mod room_path;
pub mod signin;
pub mod signup;
pub mod ssr;
pub mod state;
pub mod ws;

use std::sync::Arc;

use any_spawner::Executor;
use axum::{
    Router,
    routing::{any, get},
};
use leptos_axum::{site_pkg_dir_service, site_pkg_dir_service_route_path};
use leptos_config::get_configuration;

pub async fn application() -> Result<(), Box<dyn std::error::Error>> {
    let _ = Executor::init_tokio();

    let conf = get_configuration(None)?;
    let leptos_options = conf.leptos_options;

    let pkg_path = site_pkg_dir_service_route_path(&leptos_options);
    let pkg_service = site_pkg_dir_service(&leptos_options);

    let database_url = std::env::var("CHAT_DATABASE_URL")?;
    let db = sqlx::SqlitePool::connect(&database_url).await?;

    chat_persistence::migrate(&db).await?;

    let room_repository = Arc::new(chat_persistence::SqliteRoomRepository::new(db.clone()));
    let message_repository = Arc::new(chat_persistence::SqliteMessageRepository::new(db.clone()));

    let signup_routes = signup::routes(db.clone());
    let signin_routes = signin::routes(db.clone());
    let logout_routes = logout::routes(db.clone());

    let state = state::AppState::new(leptos_options, room_repository, message_repository);

    let guarded_room = Router::new()
        .route("/room/{room_id}", get(ssr::app_handler))
        .route_layer(axum::middleware::from_fn_with_state(
            db.clone(),
            room_auth::require_room_member,
        ));

    let guarded_ws = Router::new()
        .route("/ws/{room_id}", any(ws::ws_handler))
        // 此 pool 僅給已存在的 WS handler 重驗 Session；不改其他路由的 State。
        .layer(axum::Extension(db.clone()))
        .route_layer(axum::middleware::from_fn_with_state(
            db.clone(),
            room_auth::require_member,
        ));

    // 建立房間需要有效會員；公開列表保持可讀。
    // GET 公開；只替 POST method 加上會員 Session middleware。
    let room_methods = get(http::list_rooms)
        .merge(axum::routing::post(member_self::create_room_api).with_state(db.clone()));

    let app = Router::new()
        .route_service(&pkg_path, pkg_service)
        .route("/health", get(http::health))
        .route("/api/rooms", room_methods)
        .route("/api/rooms/{id}", get(http::get_room))
        .route("/signin", get(ssr::app_handler))
        .route("/", get(ssr::app_handler))
        .merge(guarded_room)
        .merge(guarded_ws)
        .with_state(state)
        .merge(signup_routes)
        .merge(signin_routes)
        .merge(logout_routes);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;

    println!("Server running on http://127.0.0.1:3000");

    axum::serve(listener, app).await?;

    Ok(())
}
