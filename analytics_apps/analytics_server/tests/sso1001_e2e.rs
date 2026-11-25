#![cfg(all(feature = "postgres-integration", feature = "e2e-tests"))]

use ring_test::{Browser, Process, required, wait};
use serde_json::json;
use sqlx::ConnectOptions;
use std::{net::TcpStream, path::PathBuf};

#[sqlx::test(migrations = "../migrations")]
async fn sso1001_e2e_001_expired_report_login_restores_query_and_rejects_external_return(
    pool: sqlx::PgPool,
) {
    let root = PathBuf::from(required("RING_ARTIFACTS")).join("sso1001_e2e_001");
    std::fs::create_dir_all(&root).unwrap();
    let database = pool.connect_options().to_url_lossy().to_string();
    let _server = Process::spawn(
        &required("RING_ANALYTICS_BINARY"),
        &[],
        &[
            ("ANALYTICS_DATABASE_URL", database.clone()),
            ("ANALYTICS_CHAT_TOKEN", "fixture-chat".into()),
            ("ANALYTICS_PET_TOKEN", "fixture-pet".into()),
        ],
        &root.join("server.log"),
    );
    wait("Analytics啟動", || {
        TcpStream::connect(("127.0.0.1", 3100)).ok()
    });
    let b = Browser::start("sso1001_e2e_001", None);
    let login = |pass: &str| {
        b.fill("[name=username]", "admin");
        b.fill("[name=password]", pass);
        b.click("form[action='/login'] button");
    };
    b.go("http://127.0.0.1:3100/login");
    login("admin");
    b.until(
        "首次改密碼",
        "return location.pathname==='/change-password';",
    );
    for (name, value) in [
        ("current_password", "admin"),
        ("new_password", "Synthetic-return-sso1001"),
        ("confirm_password", "Synthetic-return-sso1001"),
        ("email", "sso1001@example.test"),
    ] {
        b.fill(
            &format!("form[action='/change-password'] [name={name}]"),
            value,
        );
    }
    b.click("form[action='/change-password'] button[type=submit]");
    b.until("改密後登入", "return location.pathname==='/login';");
    login("Synthetic-return-sso1001");
    b.until("報表入口", "return location.pathname==='/dashboard';");
    sqlx::query("DELETE FROM uac_login_session")
        .execute(&pool)
        .await
        .unwrap();
    let target = "/dashboard?source=desktop_pet&event_type=%E6%B8%AC%E8%A9%A6%20%25_%26&page_size=25&page=2&start=2026-11-01&end=";
    b.go(&format!("http://127.0.0.1:3100{target}"));
    b.until("過期轉登入", "return location.pathname==='/login';");
    assert_eq!(
        b.read(
            "return document.querySelector('input[name=next]')?.value;",
            json!([])
        ),
        json!(target)
    );
    login("incorrect-password");
    b.until(
        "錯誤保留查詢",
        "return !!document.querySelector('[role=alert]')?.textContent.trim();",
    );
    assert_eq!(
        b.read(
            "return document.querySelector('input[name=next]').value;",
            json!([])
        ),
        json!(target)
    );
    tokio::time::sleep(std::time::Duration::from_secs(16)).await;
    login("Synthetic-return-sso1001");
    b.until("返回原報表", "return location.pathname==='/dashboard';");
    assert_eq!(b.read("const p=new URLSearchParams(location.search);return [p.get('source'),p.get('event_type'),p.get('page_size'),p.get('start'),p.get('end')];",json!([])),json!(["desktop_pet","測試 %_&","25","2026-11-01",""]));
    assert_eq!(b.read("return [document.querySelector('#report-start').value,document.querySelector('#report-end').value];",json!([])),json!(["2026-11-01",""]));
    b.go("http://127.0.0.1:3100/login?next=https%3A%2F%2Fevil.example");
    assert_eq!(
        b.read(
            "return document.querySelector('input[name=next]')?.value||'';",
            json!([])
        ),
        json!("")
    );
    b.save("restored-report");
}
