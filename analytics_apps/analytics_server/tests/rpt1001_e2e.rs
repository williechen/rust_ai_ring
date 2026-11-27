#![cfg(all(feature = "postgres-integration", feature = "e2e-tests"))]

use ring_test::{Browser, Process, required, wait};
use serde_json::json;
use sqlx::ConnectOptions;
use std::{net::TcpStream, path::PathBuf};

#[sqlx::test(migrations = "../migrations")]
async fn rpt1001_e2e_001_invalid_dates_can_be_corrected_without_losing_fuzzy_filters(
    pool: sqlx::PgPool,
) {
    let root = PathBuf::from(required("RING_ARTIFACTS")).join("rpt1001_e2e_001");
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
    let b = Browser::start("rpt1001_e2e_001", None);
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

    sqlx::query("INSERT INTO product_events(source_app,event_id,schema_version,event_type,occurred_at) VALUES('desktop_pet',$1,1,'pet.sleep.started','2026-11-25T00:00:00Z')").bind(uuid::Uuid::new_v4().to_string()).execute(&pool).await.unwrap();
    b.go("http://127.0.0.1:3100/dashboard?source=desktop_pet&event_type=SLEEP&page_size=25&start=2026-11-20&end=2026-11-01");
    b.until(
        "錯誤頁可直接修正",
        "return !!document.querySelector('form[aria-label=\"修正日期查詢\"]');",
    );
    assert_eq!(b.read("return ['source','event_type','page_size','start','end'].map(k=>document.querySelector('[name='+k+']').value);",json!([])),json!(["desktop_pet","SLEEP","25","2026-11-20","2026-11-01"]));
    for (width, label) in [(1366, "修正日期桌面"), (390, "修正日期窄窗")] {
        b.call("POST", "/window/rect", json!({"width":width,"height":900}))
            .unwrap();
        b.until(
            "控制項已配置",
            "return document.querySelector('#report-start').getBoundingClientRect().height>0;",
        );
        assert!(b.read("const h=['event-type','report-start','report-end'].map(k=>document.getElementById(k).getBoundingClientRect().height);return Math.max(...h)-Math.min(...h)<1&&document.documentElement.scrollWidth<=innerWidth;",json!([])).as_bool().unwrap());
        b.save(label);
    }
    b.fill("#report-end", "");
    b.click("form[aria-label='修正日期查詢'] button");
    b.until(
        "單邊日期重查",
        "return document.title!=='日期範圍無效'&&!!document.querySelector('#report-start');",
    );
    assert_eq!(b.read("return [document.querySelector('#event-type').value,document.querySelector('#report-start').value,document.querySelector('#report-end').value,location.hash];",json!([])),json!(["SLEEP","2026-11-20","",""]));
    assert!(
        b.read(
            "return document.body.textContent.includes('1 筆已提交事件');",
            json!([])
        )
        .as_bool()
        .unwrap()
    );
    b.go("http://127.0.0.1:3100/dashboard?source=desktop_pet&event_type=%E6%B8%AC%E8%A9%A6%20%25_%26&page_size=25&start=2026-11-20&end=2026-11-01");
    b.until(
        "保留字面查詢",
        "return !!document.querySelector('form[aria-label=\"修正日期查詢\"]');",
    );
    assert_eq!(
        b.read(
            "return document.querySelector('#event-type').value;",
            json!([])
        ),
        json!("測試 %_&")
    );
    b.click("a.report-export");
    b.until(
        "清除日期返回",
        "return document.title!=='日期範圍無效'&&!!document.querySelector('#report-start');",
    );
    assert_eq!(b.read("return [document.querySelector('#event-type').value,document.querySelector('#report-start').value,document.querySelector('#report-end').value,location.hash];",json!([])),json!(["測試 %_&","","",""]));
    b.save("完成");
}

#[sqlx::test(migrations = "../migrations")]
async fn rpt1001_e2e_002_forbidden_source_returns_to_authorized_report_without_leak(
    pool: sqlx::PgPool,
) {
    let root = PathBuf::from(required("RING_ARTIFACTS")).join("rpt1001_e2e_002");
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
    let b = Browser::start("rpt1001_e2e_002", None);
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

    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) SELECT id,'analytics-chat-read','deny',now() FROM uac_user WHERE lower(user_acnt)='admin'").execute(&pool).await.unwrap();
    for (source, kind) in [
        ("desktop_pet", "pet.sleep.started"),
        ("web_chat", "private.chat.event"),
    ] {
        sqlx::query("INSERT INTO product_events(source_app,event_id,schema_version,event_type,occurred_at) VALUES($1,$2,1,$3,'2026-11-26T00:00:00Z')").bind(source).bind(uuid::Uuid::new_v4().to_string()).bind(kind).execute(&pool).await.unwrap();
    }
    b.go("http://127.0.0.1:3100/dashboard?source=web_chat");
    b.until(
        "無權限頁可恢復",
        "return !!document.querySelector('a[href=\"/dashboard\"]');",
    );
    assert_eq!(
        b.read("return document.title;", json!([])),
        json!("無法檢視此來源")
    );
    assert_eq!(
        b.read(
            "return document.body.textContent.includes('private.chat.event');",
            json!([])
        ),
        json!(false)
    );
    b.call("POST", "/timeouts", json!({"script":15000}))
        .unwrap();
    // W3C async script is a browser request bridge; policy and data remain real server/PG.
    let denied=b.call("POST","/execute/async",json!({"script":"const done=arguments[arguments.length-1];Promise.all(['/dashboard?source=web_chat','/api/v1/events/web_chat/summary','/api/v1/events/web_chat/export.csv'].map(url=>fetch(url).then(async r=>[r.status,(await r.text()).includes('private.chat.event')]))).then(done);","args":[]})).unwrap();
    assert_eq!(denied, json!([[403, false], [403, false], [403, false]]));
    b.save("無來源權限");
    b.click("a[href='/dashboard']");
    b.until(
        "返回已授權來源",
        "return !!document.querySelector('select[name=source]');",
    );
    assert_eq!(
        b.read(
            "return document.querySelector('select[name=source]').value;",
            json!([])
        ),
        json!("desktop_pet")
    );
    assert_eq!(
        b.read(
            "return document.body.textContent.includes('private.chat.event');",
            json!([])
        ),
        json!(false)
    );
    assert!(
        b.read(
            "return document.body.textContent.includes('1 筆已提交事件');",
            json!([])
        )
        .as_bool()
        .unwrap()
    );
    // Removing the final source grant must not create a recovery loop or leak data.
    sqlx::query("INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at) SELECT id,'analytics-pet-read','deny',now() FROM uac_user WHERE lower(user_acnt)='admin'").execute(&pool).await.unwrap();
    b.go("http://127.0.0.1:3100/dashboard");
    b.until("所有來源未授權明示","return document.title==='無法檢視此來源'&&document.body.textContent.includes('請聯絡管理員');");
    b.save("全部未授權");
}
