#![cfg(all(feature = "ssr", feature = "e2e-tests"))]

use ring_test::{Browser, Process, required, wait};
use serde_json::json;
use sqlx::ConnectOptions;
use std::{net::TcpStream, path::PathBuf};

#[sqlx::test(migrations = "../migrations")]
async fn rdi2001_e2e_001_search_enter_respects_ime_modifiers_and_category(pool: sqlx::SqlitePool) {
    let root = PathBuf::from(required("RING_ARTIFACTS")).join("rdi2001_e2e_001");
    std::fs::create_dir_all(&root).unwrap();
    let options = pool.connect_options();
    let database = options
        .as_ref()
        .clone()
        .filename(std::path::absolute(options.get_filename()).unwrap())
        .to_url_lossy()
        .to_string();
    let _server = Process::spawn(
        &required("RING_CHAT_BINARY"),
        &[],
        &[
            ("CHAT_DATABASE_URL", database),
            ("LEPTOS_SITE_ROOT", required("RING_CHAT_SITE")),
            ("LEPTOS_OUTPUT_NAME", "rust_ai_chat".into()),
            ("LEPTOS_SITE_PKG_DIR", "pkg".into()),
            ("LEPTOS_SITE_ADDR", "127.0.0.1:3000".into()),
        ],
        &root.join("server.log"),
    );
    wait("Chat啟動", || {
        TcpStream::connect(("127.0.0.1", 3000)).ok()
    });
    sqlx::query("INSERT INTO codemap(category,code,value) VALUES('room_category','rdi_keyboard','鍵盤分類')").execute(&pool).await.unwrap();
    for name in ["Rust甲", "Rust乙", "Other"] {
        sqlx::query("INSERT INTO rooms(id,category_room_id,name,desc,created_at,updated_at) VALUES(?,'rdi_keyboard',?,'',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP)").bind(uuid::Uuid::new_v4().to_string()).bind(name).execute(&pool).await.unwrap();
    }
    let b = Browser::start("rdi2001_e2e_001", None);
    b.go("http://127.0.0.1:3000/");
    b.until("目錄水合就緒","return !!document.querySelector('[data-category=rdi_keyboard]')&&!document.querySelector('.directory-search button[type=submit]').disabled;");
    b.click("[data-category=rdi_keyboard]");
    b.until("分類資料就緒","return document.querySelectorAll('.room-card').length===3&&!document.querySelector('.directory-search button[type=submit]').disabled;");
    b.act_once("window.directoryRequests=[];window.originalFetch=window.fetch;window.fetch=function(input,init){const url=new URL(typeof input==='string'?input:input.url,location.origin);if(url.pathname==='/api/rooms')window.directoryRequests.push(url.href);return window.originalFetch.call(this,input,init);};",json!([]));
    b.fill("#directory-query", "RuSt");
    let el = b
        .call(
            "POST",
            "/element",
            json!({"using":"css selector","value":"#directory-query"}),
        )
        .unwrap();
    let id = el["element-6066-11e4-a52e-4f735466cecf"].as_str().unwrap();
    b.call("POST", &format!("/element/{id}/click"), json!({}))
        .unwrap();
    b.call("POST","/actions",json!({"actions":[{"type":"key","id":"keyboard","actions":[{"type":"keyDown","value":"\u{e008}"},{"type":"keyDown","value":"\u{e007}"},{"type":"keyUp","value":"\u{e007}"},{"type":"keyUp","value":"\u{e008}"}]}]})).unwrap();
    b.act_once(
        "window.keyboardSettled=false;setTimeout(()=>window.keyboardSettled=true,150);",
        json!([]),
    );
    b.until("鍵盤事件完成", "return window.keyboardSettled;");
    assert_eq!(
        b.read("return window.directoryRequests.length;", json!([])),
        json!(0),
        "Shift+Enter不能發出名稱查詢"
    );
    for extra in [
        "isComposing:true",
        "keyCode:229",
        "ctrlKey:true",
        "altKey:true",
        "metaKey:true",
    ] {
        b.act_once(&format!("const e=new KeyboardEvent('keydown',{{key:'Enter',bubbles:true,cancelable:true,{extra}}});document.querySelector('#directory-query').dispatchEvent(e);window.imeBlocked=e.defaultPrevented;"),json!([]));
        assert_eq!(b.read("return window.imeBlocked;", json!([])), json!(true));
    }
    b.call(
        "POST",
        &format!("/element/{id}/value"),
        json!({"text":"\u{e007}","value":["\u{e007}"]}),
    )
    .unwrap();
    b.until("真HTTP模糊查詢","return document.querySelectorAll('.room-card').length===2&&!document.querySelector('.directory-search button[type=submit]').disabled;");
    assert_eq!(
        b.read("return window.directoryRequests.length;", json!([])),
        json!(1)
    );
    assert_eq!(b.read("return document.querySelector('[data-category=rdi_keyboard]').getAttribute('aria-pressed');",json!([])),json!("true"));
    assert_eq!(
        b.read(
            "return document.querySelector('#directory-query').value;",
            json!([])
        ),
        json!("RuSt")
    );
    assert!(b.read("return [...document.querySelectorAll('.room-card h3')].every(n=>n.textContent.includes('Rust'));",json!([])).as_bool().unwrap());
    b.save("分類與鍵盤查詢");
}
