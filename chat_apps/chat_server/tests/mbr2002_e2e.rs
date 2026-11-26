#![cfg(all(feature = "ssr", feature = "e2e-tests"))]

use ring_test::{Browser, Process, required, wait};
use serde_json::json;
use sqlx::ConnectOptions;
use std::{net::TcpStream, path::PathBuf};

#[sqlx::test(migrations = "../migrations")]
async fn mbr2002_e2e_001_profile_entry_save_cancel_and_session_scope(pool: sqlx::SqlitePool) {
    let root = PathBuf::from(required("RING_ARTIFACTS")).join("mbr2002_e2e_001");
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
    let b = Browser::start("mbr2002_e2e_001", None);

    b.go("http://127.0.0.1:3000/signup");
    for (name, value) in [
        ("username", "rust_mbr2002"),
        ("password", "Synthetic-profile-mbr2002"),
        ("display_name", "修改前"),
        ("email", "mbr2002@example.test"),
    ] {
        b.fill(&format!("[name={name}]"), value);
    }
    b.click("form[action='/signup'] button");
    b.until("註冊完成", "return location.pathname==='/signup/complete';");
    b.go("http://127.0.0.1:3000/login");
    b.fill("[name=username]", "rust_mbr2002");
    b.fill("[name=password]", "Synthetic-profile-mbr2002");
    b.click("form[action='/login'] button");
    b.until("個人管理", "return location.pathname==='/account';");
    b.until(
        "本人資料入口",
        "return !!document.querySelector('a[href=\"/account/profile\"]');",
    );
    b.click("a[href='/account/profile']");
    b.until(
        "預填本人",
        "return document.querySelector('#profile-display-name')?.value==='修改前';",
    );
    b.fill("#profile-display-name", "尚未儲存");
    b.click("a[href='/account']");
    b.until("取消回管理", "return location.pathname==='/account';");
    let name: String =
        sqlx::query_scalar("SELECT user_name FROM mbr_user WHERE user_acnt='rust_mbr2002'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(name, "修改前");
    b.click("a[href='/account/profile']");
    b.until(
        "重新預填",
        "return document.querySelector('#profile-display-name')?.value==='修改前';",
    );
    b.fill("#profile-display-name", "已儲存會員");
    b.fill("#profile-email", "mbr2002-updated@example.test");
    b.click("form[action='/account/profile'] button");
    b.until(
        "儲存成功回饋",
        "return document.querySelector('[role=alert]')?.textContent==='本人資料已儲存';",
    );
    let data: (String, String) =
        sqlx::query_as("SELECT user_name,user_email FROM mbr_user WHERE user_acnt='rust_mbr2002'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        data,
        ("已儲存會員".into(), "mbr2002-updated@example.test".into())
    );
    b.go("http://127.0.0.1:3000/account");
    b.click("a[href='/account/profile']");
    b.until(
        "重讀已存資料",
        "return document.querySelector('#profile-display-name')?.value==='已儲存會員';",
    );
    sqlx::query("DELETE FROM mbr_session")
        .execute(&pool)
        .await
        .unwrap();
    b.go("http://127.0.0.1:3000/account/profile");
    assert!(
        !b.read(
            "return !!document.querySelector('#profile-display-name');",
            json!([])
        )
        .as_bool()
        .unwrap()
    );
    b.save("完成");
}
