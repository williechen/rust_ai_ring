#![cfg(all(feature = "ssr", feature = "e2e-tests"))]

use ring_test::{Browser, Process, required, wait};
use serde_json::json;
use sqlx::ConnectOptions;
use std::{net::TcpStream, path::PathBuf};

#[sqlx::test(migrations = "../migrations")]
async fn msg2001_e2e_001_reconnect_keeps_draft_recovers_history_and_rechecks_session(
    pool: sqlx::SqlitePool,
) {
    let root = PathBuf::from(required("RING_ARTIFACTS")).join("msg2001_e2e_001");
    std::fs::create_dir_all(&root).unwrap();
    let options = pool.connect_options();
    let database = options
        .as_ref()
        .clone()
        .filename(std::path::absolute(options.get_filename()).unwrap())
        .to_url_lossy()
        .to_string();
    let env = [
        ("CHAT_DATABASE_URL", database.clone()),
        ("LEPTOS_SITE_ROOT", required("RING_CHAT_SITE")),
        ("LEPTOS_OUTPUT_NAME", "rust_ai_chat".into()),
        ("LEPTOS_SITE_PKG_DIR", "pkg".into()),
        ("LEPTOS_SITE_ADDR", "127.0.0.1:3000".into()),
    ];
    let start = || {
        Process::spawn(
            &required("RING_CHAT_BINARY"),
            &[],
            &env,
            &root.join("server.log"),
        )
    };
    let mut server = Some(start());
    wait("Chat啟動", || {
        TcpStream::connect(("127.0.0.1", 3000)).ok()
    });
    let room = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO codemap(category,code,value) VALUES('room_category','msg2001_e2e','測試分類')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES(?,'msg2001_e2e','測試房間','',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").bind(&room).execute(&pool).await.unwrap();
    let b = Browser::start("msg2001_e2e_001", None);
    b.go("http://127.0.0.1:3000/signup");
    for (name, value) in [
        ("username", "rust_msg2001"),
        ("password", "Synthetic-reconnect-msg2001"),
        ("display_name", "測試會員"),
        ("email", "rust_msg2001@example.test"),
    ] {
        b.fill(&format!("[name={name}]"), value);
    }
    b.click("form[action='/signup'] button");
    b.until("註冊完成", "return location.pathname==='/signup/complete';");
    b.go(&format!(
        "http://127.0.0.1:3000/login?next=%2Froom%2F{room}"
    ));
    b.until(
        "登入表單水合",
        "return !!document.querySelector('form[action=\"/login\"] button');",
    );
    b.fill("[name=username]", "rust_msg2001");
    b.fill("[name=password]", "Synthetic-reconnect-msg2001");
    b.click("form[action='/login'] button");
    b.until("房間連線成功","return !!document.querySelector('.chat-composer textarea')&&document.querySelector('.conversation-heading').textContent.includes('已連線');");
    b.fill(".chat-composer textarea", "離線後仍保留的草稿");
    drop(server.take());
    b.until(
        "重新連線入口",
        "return !!document.querySelector('[data-action=reconnect]');",
    );
    assert_eq!(
        b.read(
            "return document.querySelector('.chat-composer textarea').value;",
            json!([])
        ),
        json!("離線後仍保留的草稿")
    );
    sqlx::query("INSERT INTO messages(id,room_id,author_kind,content) VALUES(?,?,'anonymous_user','離線時已提交的歷史')").bind(uuid::Uuid::new_v4().to_string()).bind(&room).execute(&pool).await.unwrap();
    server = Some(start());
    wait("Chat重啟", || {
        TcpStream::connect(("127.0.0.1", 3000)).ok()
    });
    b.click("[data-action=reconnect]");
    b.until("重連及補讀歷史","return document.querySelector('.conversation-heading').textContent.includes('已連線')&&document.querySelector('.message-history').textContent.includes('離線時已提交的歷史');");
    assert_eq!(
        b.read(
            "return document.querySelector('.chat-composer textarea').value;",
            json!([])
        ),
        json!("離線後仍保留的草稿")
    );
    b.click(".chat-composer button");
    b.until("真WS送出","return document.querySelector('.message-history').textContent.includes('離線後仍保留的草稿');");
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM messages WHERE room_id=? AND content='離線後仍保留的草稿'",
    )
    .bind(&room)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(n, 1);
    // 撤銷後重連不得繞過正式握手授權，草稿仍保留。
    b.fill(".chat-composer textarea", "撤銷後不得送出");
    sqlx::query("DELETE FROM mbr_session")
        .execute(&pool)
        .await
        .unwrap();
    drop(server.take());
    server = Some(start());
    wait("撤銷後Chat啟動", || {
        TcpStream::connect(("127.0.0.1", 3000)).ok()
    });
    b.until(
        "再次可重連",
        "return !!document.querySelector('[data-action=reconnect]');",
    );
    b.click("[data-action=reconnect]");
    b.until(
        "拒絕過期Session",
        "const n=document.querySelector('[data-action=reconnect]');return !!n&&!n.disabled;",
    );
    assert_eq!(
        b.read(
            "return document.querySelector('.chat-composer textarea').value;",
            json!([])
        ),
        json!("撤銷後不得送出")
    );
    b.save("reconnect");
    drop(server);
}
