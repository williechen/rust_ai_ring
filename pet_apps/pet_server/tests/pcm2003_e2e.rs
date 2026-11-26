#![cfg(feature = "e2e-tests")]

use ring_test::{Browser, required};
use serde_json::json;
use sqlx::ConnectOptions;
use std::path::PathBuf;

#[sqlx::test(migrations = "../migrations")]
async fn pcm2003_e2e_001_send_preserves_new_draft_and_rejects_previous_selection_response(
    pool: sqlx::SqlitePool,
) {
    let root = PathBuf::from(required("RING_ARTIFACTS")).join("pcm2003_e2e_001");
    std::fs::create_dir_all(&root).unwrap();
    let options = pool.connect_options();
    let database = options
        .as_ref()
        .clone()
        .filename(std::path::absolute(options.get_filename()).unwrap())
        .to_url_lossy()
        .to_string();
    let store = pet_server::pet_store::PetStore { pool: pool.clone() };
    let a = store.create("計時甲").await.unwrap();
    let other = store.create("計時乙").await.unwrap();
    sqlx::query("UPDATE pet_profile SET is_default_closure=1")
        .execute(&pool)
        .await
        .unwrap();
    let b = Browser::start_with_env(
        "pcm2003_e2e_001",
        Some(&required("RING_PET_BINARY")),
        &[
            ("PET_DATABASE_URL", database),
            ("PET_AUTHORIZED_ROOT", root.to_string_lossy().into_owned()),
            ("RING_TEST_CLOCK", "1".into()),
            ("WEBKIT_DISABLE_DMABUF_RENDERER", "1".into()),
            ("WEBKIT_DISABLE_COMPOSITING_MODE", "1".into()),
        ],
    );
    b.until(
        "控制台就緒",
        "return !!document.querySelector('.control-pet-picker select');",
    );
    b.until(
        "清單包含測試寵物",
        &format!(
            "return !!document.querySelector('.control-pet-picker option[value=\"{}\"]');",
            a.id
        ),
    );
    b.act_once("const n=document.querySelector('.control-pet-picker select');n.value=arguments[0];n.dispatchEvent(new Event('change',{bubbles:true}));",json!([a.id]));

    b.until(
        "聊天輸入",
        "return !!document.querySelector('.chat-bubble__input');",
    );
    b.act_once("window.originalChat=window.chatWithPet;window.pendingChat=[];window.chatWithPet=(id,text)=>window.originalChat(id,text).then(value=>new Promise(resolve=>window.pendingChat.push({resolve,value,id})));",json!([]));
    b.fill(".chat-bubble__input", "第一個訊息");
    b.click(".chat-bubble__send");
    b.until("Native回應已就緒", "return window.pendingChat.length===1;");
    b.fill(".chat-bubble__input", "還沒送出的下一句");
    b.act_once(
        "const p=window.pendingChat.shift();p.resolve(p.value);",
        json!([]),
    );
    b.until(
        "第一次送出完成",
        "return !document.querySelector('.chat-bubble__send').disabled;",
    );
    assert_eq!(
        b.read(
            "return document.querySelector('.chat-bubble__input').value;",
            json!([])
        ),
        json!("還沒送出的下一句")
    );
    b.click(".chat-bubble__send");
    b.until(
        "第二次Native回應就緒",
        "return window.pendingChat.length===1;",
    );
    for id in [&other.id, &a.id] {
        b.act_once("const n=document.querySelector('.control-pet-picker select');n.value=arguments[0];n.dispatchEvent(new Event('change',{bubbles:true}));",json!([id]));
        b.until(
            "新選取草稿重置",
            "return document.querySelector('.chat-bubble__input').value==='';",
        );
        b.fill(".chat-bubble__input", "新選取的草稿");
        b.until(
            "新選取可送出",
            "return !document.querySelector('.chat-bubble__send').disabled;",
        );
    }
    b.act_once("window.releaseReady=false;const p=window.pendingChat.shift();p.resolve(p.value);setTimeout(()=>window.releaseReady=true,100);",json!([]));
    b.until("舊回應已釋放", "return window.releaseReady;");
    assert_eq!(
        b.read(
            "return document.querySelector('.chat-bubble__input').value;",
            json!([])
        ),
        json!("新選取的草稿")
    );
    assert!(
        b.read(
            "return !document.querySelector('.chat-bubble__response');",
            json!([])
        )
        .as_bool()
        .unwrap()
    );
    b.act_once("window.chatWithPet=window.originalChat;", json!([]));
    b.click(".chat-bubble__send");
    b.until(
        "目前選取真回應",
        "return !!document.querySelector('.chat-bubble__response')?.textContent.trim();",
    );
    assert_eq!(
        b.read(
            "return document.querySelector('.chat-bubble__input').value;",
            json!([])
        ),
        json!("")
    );
    b.save("完成");
}
