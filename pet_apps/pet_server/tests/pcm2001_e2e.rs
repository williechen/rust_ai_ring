#![cfg(feature = "e2e-tests")]

use ring_test::{Browser, required};
use serde_json::json;
use sqlx::ConnectOptions;
use std::path::PathBuf;

#[sqlx::test(migrations = "../migrations")]
async fn pcm2001_e2e_001_creation_preserves_next_name_draft_and_commits_once(
    pool: sqlx::SqlitePool,
) {
    let root = PathBuf::from(required("RING_ARTIFACTS")).join("pcm2001_e2e_001");
    std::fs::create_dir_all(&root).unwrap();
    let options = pool.connect_options();
    let database = options
        .as_ref()
        .clone()
        .filename(std::path::absolute(options.get_filename()).unwrap())
        .to_url_lossy()
        .to_string();
    let b = Browser::start_with_env(
        "pcm2001_e2e_001",
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
        "return !!document.querySelector('#pet-management-tab');",
    );
    b.click("#pet-management-tab");
    b.until(
        "管理區就緒",
        "return !document.querySelector('#pet-management').hidden;",
    );
    b.act_once("window.originalCreate=window.createPet;window.pendingCreate=[];window.createCalls=0;window.createPet=name=>{window.createCalls++;return window.originalCreate(name).then(value=>new Promise(resolve=>window.pendingCreate.push({resolve,value})));};",json!([]));
    b.fill("#pet-management > label input", "建立甲");
    b.click("#pet-management > button");
    b.until("真Native已建立", "return window.pendingCreate.length===1;");
    b.fill("#pet-management > label input", "下一隻乙");
    b.act_once(
        "const p=window.pendingCreate.shift();p.resolve(p.value);",
        json!([]),
    );
    b.until("第一次建立完成","return !document.querySelector('#pet-management > button').disabled&&document.querySelector('#pet-management').textContent.includes('已建立：建立甲');");
    assert_eq!(
        b.read(
            "return document.querySelector('#pet-management > label input').value;",
            json!([])
        ),
        json!("下一隻乙"),
        "建立完成不得清掉期間新名稱草稿"
    );
    let first: i64 = sqlx::query_scalar("SELECT count(*) FROM pet_profile WHERE name='建立甲'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(first, 1);
    assert_eq!(b.read("return window.createCalls;", json!([])), json!(1));
    b.act_once("window.createPet=window.originalCreate;", json!([]));
    b.click("#pet-management > button");
    b.until("第二次建立完成","return !document.querySelector('#pet-management > button').disabled&&document.querySelector('#pet-management').textContent.includes('已建立：下一隻乙');");
    let rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT id,name FROM pet_profile WHERE name IN ('建立甲','下一隻乙') ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 2);
    assert_ne!(rows[0].0, rows[1].0);
    assert_eq!(
        b.read(
            "return document.querySelector('#pet-management > label input').value;",
            json!([])
        ),
        json!("")
    );
    b.save("建立與新草稿保留");
}
