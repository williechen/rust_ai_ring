use super::*;
use axum::http::header::COOKIE;
use chat_application::repository::MessageRepository;
use chat_domain::MessageAuthor;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::sync::Arc;
use tokio::time::{Instant, sleep, timeout};

const ROOM: &str = "write-room";
const MEMBER: &str = "write-member";

async fn fixture(pool: &SqlitePool) -> (AppState, HeaderMap) {
    sqlx::query(
        "INSERT INTO rooms VALUES
         ('write-room','default-category','測試房間','',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO mbr_user
         (id,user_acnt,user_pd,user_email,user_name,user_status,is_init_pd,created_at,updated_at)
         VALUES('write-member','writer','hash','writer@example.invalid','測試會員',
                'active',FALSE,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
    )
    .execute(pool)
    .await
    .unwrap();
    let raw = "a".repeat(64);
    sqlx::query(
        "INSERT INTO mbr_session (id,mbr_user_id,token_hash,expires_at)
         VALUES ('write-session','write-member',?1,unixepoch()+600)",
    )
    .bind(token_digest(&raw))
    .execute(pool)
    .await
    .unwrap();
    let state = AppState::new(
        leptos_config::LeptosOptions::builder()
            .output_name("fixture")
            .build(),
        Arc::new(chat_persistence::SqliteRoomRepository::new(pool.clone())),
        Arc::new(chat_persistence::SqliteMessageRepository::new(pool.clone())),
    );
    let mut headers = HeaderMap::new();
    headers.insert(COOKIE, format!("chat_session={raw}").parse().unwrap());
    assert_eq!(
        authorize_socket(&state, pool, &headers, ROOM)
            .await
            .unwrap(),
        MEMBER
    );
    (state, headers)
}

async fn send(state: &AppState, pool: &SqlitePool, headers: &HeaderMap) -> bool {
    persist_member_event(
        state,
        pool,
        headers,
        ROOM.into(),
        "測試訊息".into(),
        MEMBER.into(),
    )
    .await
    .unwrap()
}

fn options(path: &std::path::Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(10))
}

#[sqlx::test(migrations = "../migrations")]
async fn session_expires_while_waiting_for_writer(pool: SqlitePool) {
    let path: String =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name='main'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let writer = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options(path.as_ref()))
        .await
        .unwrap();
    let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&writer)
        .await
        .unwrap();
    assert_eq!(journal, "wal");
    let (state, headers) = fixture(&pool).await;
    let (_, mut events) = state.room_hub.subscribe(ROOM.into());
    let expires: i64 =
        sqlx::query_scalar("UPDATE mbr_session SET expires_at=unixepoch()+3 RETURNING expires_at")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut lock = pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
    let (sqlite, now): (String, i64) = sqlx::query_as("SELECT sqlite_version(), unixepoch()")
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    assert!(now < expires);
    assert_eq!(
        authorize_socket(&state, &writer, &headers, ROOM)
            .await
            .unwrap(),
        MEMBER
    );

    let started = Instant::now();
    let attempt = send(&state, &writer, &headers);
    tokio::pin!(attempt);
    assert!(
        timeout(Duration::from_millis(200), &mut attempt)
            .await
            .is_err()
    );
    let unlocked_at = timeout(Duration::from_secs(5), async {
        loop {
            let now: i64 = sqlx::query_scalar("SELECT unixepoch()")
                .fetch_one(&mut *lock)
                .await
                .unwrap();
            if now >= expires {
                break now;
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    lock.commit().await.unwrap();
    let saved = timeout(Duration::from_secs(5), attempt).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    let published = events.try_recv().is_ok();
    eprintln!(
        "SQLite {sqlite}｜journal_mode {journal}｜開始 {now}｜到期 {expires}｜解鎖 {unlocked_at}｜等待 {} ms｜寫入 {count}｜推播 {published}",
        started.elapsed().as_millis()
    );
    assert_eq!((saved, count, published), (false, 0, false));
}

#[sqlx::test(migrations = "../migrations")]
async fn committed_member_message_publishes_once_and_survives_reopen(pool: SqlitePool) {
    let (state, headers) = fixture(&pool).await;
    let before: i64 = sqlx::query_scalar("SELECT expires_at FROM mbr_session")
        .fetch_one(&pool)
        .await
        .unwrap();
    let (_, mut events) = state.room_hub.subscribe(ROOM.into());
    let (_, mut other) = state.room_hub.subscribe("other-room".into());
    assert!(send(&state, &pool, &headers).await);
    let ServerEvent::MessageCreated(message) = events.try_recv().unwrap() else {
        panic!("預期收到已寫入的訊息");
    };
    assert_eq!(message.room_id, ROOM);
    assert_eq!(message.content, "測試訊息");
    assert_eq!(
        message.author,
        MessageAuthor::User {
            user_id: MEMBER.into()
        }
    );
    assert!(matches!(
        events.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
    assert!(matches!(
        other.try_recv(),
        Err(broadcast::error::TryRecvError::Empty)
    ));
    let path: String =
        sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name='main'")
            .fetch_one(&pool)
            .await
            .unwrap();
    pool.close().await;
    let reopened = SqlitePoolOptions::new()
        .connect_with(options(path.as_ref()))
        .await
        .unwrap();
    let repo = chat_persistence::SqliteMessageRepository::new(reopened.clone());
    let stored = repo.list_by_room(ROOM).await.unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, message.id);
    assert_eq!(stored[0].author, message.author);
    assert_eq!(stored[0].content, message.content);
    let after: i64 = sqlx::query_scalar("SELECT expires_at FROM mbr_session")
        .fetch_one(&reopened)
        .await
        .unwrap();
    assert_eq!(before, after);
}

#[sqlx::test(migrations = "../migrations")]
async fn authorization_changes_after_precheck_fail_closed(pool: SqlitePool) {
    let (state, headers) = fixture(&pool).await;
    let (_, mut events) = state.room_hub.subscribe(ROOM.into());
    for (deny, restore) in [
        (
            "INSERT INTO mbr_exclude (id,mbr_user_id,room_id) VALUES ('excluded','write-member','write-room')",
            "DELETE FROM mbr_exclude",
        ),
        (
            "UPDATE mbr_user SET user_status='disabled'",
            "UPDATE mbr_user SET user_status='active'",
        ),
        (
            "UPDATE mbr_user SET is_init_pd=TRUE",
            "UPDATE mbr_user SET is_init_pd=FALSE",
        ),
        (
            "DELETE FROM rooms",
            "INSERT INTO rooms VALUES ('write-room','default-category','測試房間','',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)",
        ),
        ("DELETE FROM mbr_session", "SELECT 1"),
    ] {
        assert_eq!(
            authorize_socket(&state, &pool, &headers, ROOM)
                .await
                .unwrap(),
            MEMBER
        );
        sqlx::query(deny).execute(&pool).await.unwrap();
        assert!(!send(&state, &pool, &headers).await, "{deny}");
        assert!(matches!(
            events.try_recv(),
            Err(broadcast::error::TryRecvError::Empty)
        ));
        sqlx::query(restore).execute(&pool).await.unwrap();
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
