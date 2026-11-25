#![cfg(feature = "e2e-tests")]

use ring_test::{Browser, required};
use serde_json::json;
use sqlx::ConnectOptions;
use std::path::PathBuf;

#[sqlx::test(migrations = "../migrations")]
async fn pml2001_e2e_001_idle_eligibility_five_minutes_then_random_five_to_fifteen_and_response_reset(
    pool: sqlx::SqlitePool,
) {
    let root = PathBuf::from(required("RING_ARTIFACTS")).join("pml2001_e2e_001");
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
        "pml2001_e2e_001",
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
        "活動已載入",
        "const n=document.querySelector('.pet-workspace .control-primary');return n&&!n.disabled;",
    );
    b.click(".pet-workspace .control-primary");
    b.until("出籠","return document.querySelector('.pet-workspace .pet-status-chip').textContent.includes('已出籠');");
    let invoke = |cmd: &str, args: serde_json::Value| {
        let key = format!("nativeResult_{}", uuid::Uuid::new_v4());
        b.act_once("const key=arguments[2];window[key]={pending:true};window.__TAURI__.core.invoke(arguments[0],arguments[1]).then(v=>window[key]={ok:v},e=>window[key]={error:String(e)});",json!([cmd,args,key]));
        b.until(
            "IPC完成",
            &format!("const r=window[{}];return !!r&&!r.pending;", json!(key)),
        );
        let result = b.read("return window[arguments[0]];", json!([key]));
        assert!(result.get("error").is_none(), "{result}");
        result["ok"].clone()
    };
    let status = invoke("pet_idle_status", json!({"id":a.id}));
    assert_eq!(status["selection_revision"], 0);
    let flags: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pet_activity WHERE pet_id=? AND is_activity=1")
            .bind(&a.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(flags, 0, "出籠不可暗中開啟候選");
    let status = invoke("advance_pet_idle_clock", json!({"id":a.id,"seconds":299}));
    assert_eq!(status["selection_revision"], 0);
    let status = invoke("advance_pet_idle_clock", json!({"id":a.id,"seconds":1}));
    assert_eq!(status["selection_revision"], 1);
    assert!([json!("idle"), json!("sleep")].contains(&status["action"]));
    let delay = status["next_in_seconds"].as_u64().unwrap();
    assert!((300..=900).contains(&delay));
    b.click("input[data-idle-choice=walk]");
    b.until(
        "候選資格儲存完成",
        "return !document.querySelector('.pet-workspace .control-primary').disabled;",
    );
    let chosen:i64=sqlx::query_scalar("SELECT count(*) FROM pet_activity WHERE pet_id=? AND activity_type='walk' AND is_activity=1").bind(&a.id).fetch_one(&pool).await.unwrap();
    assert_eq!(chosen, 1);
    let current = invoke("pet_idle_status", json!({"id":a.id}));
    assert_eq!(current["selection_revision"], 1, "候選開關不立刻挑選活動");
    let next = invoke("advance_pet_idle_clock", json!({"id":a.id,"seconds":delay}));
    assert!([json!("idle"), json!("sleep"), json!("walk")].contains(&next["action"]));
    assert!((300..=900).contains(&next["next_in_seconds"].as_u64().unwrap()));
    b.click("input[data-idle-choice=sit]");
    b.until(
        "候選資格儲存完成",
        "return !document.querySelector('.pet-workspace .control-primary').disabled;",
    );
    let chosen:i64=sqlx::query_scalar("SELECT count(*) FROM pet_activity WHERE pet_id=? AND activity_type='sit' AND is_activity=1").bind(&a.id).fetch_one(&pool).await.unwrap();
    assert_eq!(chosen, 1);
    let walking:i64=sqlx::query_scalar("SELECT count(*) FROM pet_activity WHERE pet_id=? AND activity_type='walk' AND is_activity=1").bind(&a.id).fetch_one(&pool).await.unwrap();
    assert_eq!(walking, 0);
    let still = invoke("pet_idle_status", json!({"id":a.id}));
    assert_eq!(still["selection_revision"], next["selection_revision"]);
    b.fill(".chat-bubble__input", "真實回應重置計時");
    b.click(".chat-bubble__send");
    b.until("Mock回應完成","return document.querySelector('.control-chat').textContent.includes('mock: 真實回應重置計時');");
    let reset = invoke("pet_idle_status", json!({"id":a.id}));
    assert_eq!(reset["next_in_seconds"], 300);
    assert_eq!(reset["selection_revision"], 0);
    let untouched = invoke("pet_idle_status", json!({"id":other.id}));
    assert_eq!(untouched["outside"], false);
    b.click("input[data-idle-choice=none]");
    b.until(
        "清除候選完成",
        "return !document.querySelector('.pet-workspace .control-primary').disabled;",
    );
    let choices: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pet_activity WHERE pet_id=? AND is_activity=1")
            .bind(&a.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(choices, 0);
    b.click(".pet-workspace .control-primary");
    b.until("回籠","return document.querySelector('.pet-workspace .pet-status-chip').textContent.includes('已回籠');");
    let closed = invoke("pet_idle_status", json!({"id":a.id}));
    assert_eq!(closed["outside"], false);
    assert!(closed["action"].is_null());
    b.save("idle-schedule");
}
