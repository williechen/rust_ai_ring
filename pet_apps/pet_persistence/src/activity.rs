use sqlx::{SqliteConnection, SqlitePool};

pub const WALK: &str = "walk"; //散步
pub const SIT: &str = "sit"; //坐下

#[derive(Debug, Clone, serde::Serialize)]
pub struct ActivityStatus {
    pub outside: bool,
    pub roaming: bool,
    pub default_caged: bool,
}

#[derive(Debug, Clone)]
pub struct ActivitySnapshot {
    values: Vec<(String, Option<i64>)>,
    selected: Option<String>,
}

async fn local(conn: &mut SqliteConnection, id: &str) -> Result<(), String> {
    let found: bool = sqlx::query_scalar(
        r#"SELECT EXISTS(
            SELECT 1 
            FROM pet_profile 
            WHERE id=?1 
                AND owner_id='local-owner'
        )"#,
    )
    .bind(id)
    .fetch_one(conn)
    .await
    .map_err(|_| "無法驗證寵物".to_owned())?;

    if found {
        Ok(())
    } else {
        Err("找不到寵物".into())
    }
}

/// Shared by profile creation and legacy write paths, inside the caller's transaction.
/// Repeated operations never add a duplicate basic activity; other types are preserved.
pub async fn ensure_pair(conn: &mut SqliteConnection, id: &str) -> Result<(), String> {
    local(conn, id).await?;

    for kind in [WALK, SIT] {
        sqlx::query(
            r#"INSERT INTO pet_activity(id
                ,pet_id
                ,owner_id
                ,activity_type
                ,is_activity
                ,is_use_ai
            ) 
            SELECT ?1
                ,?2
                ,'local-owner'
                ,?3
                ,0
                ,0 
            WHERE NOT EXISTS(
                SELECT 1 
                FROM pet_activity 
                WHERE pet_id=?2 
                    AND owner_id='local-owner' 
                    AND activity_type=?3
            )"#,
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(id)
        .bind(kind)
        .execute(&mut *conn)
        .await
        .map_err(|_| "無法建立基本活動".to_owned())?;
    }

    Ok(())
}

async fn normalize(conn: &mut SqliteConnection, id: &str) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE pet_activity 
        SET is_activity=0
            ,updated_at=CURRENT_TIMESTAMP 
        WHERE pet_id=?1 
            AND owner_id='local-owner' 
            AND activity_type=?2 
            AND is_activity=1 
            AND EXISTS(
                SELECT 1 
                FROM pet_activity s 
                WHERE s.pet_id=?1 
                    AND s.owner_id='local-owner' 
                    AND s.activity_type=?3 
                    AND s.is_activity=1
            )"#,
    )
    .bind(id)
    .bind(WALK)
    .bind(SIT)
    .execute(conn)
    .await
    .map_err(|_| "無法保存坐下優先狀態".to_owned())?;

    Ok(())
}

async fn status_in(conn: &mut SqliteConnection, id: &str) -> Result<ActivityStatus, String> {
    local(&mut *conn, id).await?;

    let (outside, walk, sit): (bool, bool, bool) = sqlx::query_as(
        r#"SELECT EXISTS(
            SELECT 1 
            FROM pet_activity 
            WHERE pet_id=?1 
                AND owner_id='local-owner' 
                AND is_activity=1
        )
        ,EXISTS(
            SELECT 1 
            FROM pet_activity 
            WHERE pet_id=?1 
                AND owner_id='local-owner' 
                AND activity_type=?2 
                AND is_activity=1
        )
        ,EXISTS(
            SELECT 1 
            FROM pet_activity 
            WHERE pet_id=?1 
                AND owner_id='local-owner' 
                AND activity_type=?3 
                AND is_activity=1
        )"#,
    )
    .bind(id)
    .bind(WALK)
    .bind(SIT)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| "無法讀取活動狀態".to_owned())?;

    if walk && sit {
        normalize(&mut *conn, id).await?;
    }

    let pref: i64 = sqlx::query_scalar(
        r#"SELECT COALESCE(is_default_closure,0) 
        FROM pet_profile 
        WHERE id=?1 
            AND owner_id='local-owner'"#,
    )
    .bind(id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| "無法讀取預設資格".to_owned())?;

    if !matches!(pref, 0 | 1) {
        return Err("預設資格資料不合法".into());
    }

    Ok(ActivityStatus {
        outside,
        roaming: walk && !sit,
        default_caged: pref == 1,
    })
}

pub async fn status(pool: &SqlitePool, id: &str) -> Result<ActivityStatus, String> {
    let mut tx = pool.begin().await.map_err(|_| "無法讀取活動".to_owned())?;

    let state = status_in(&mut tx, id).await?;

    tx.commit()
        .await
        .map_err(|_| "活動讀取提交失敗".to_owned())?;
    Ok(state)
}

pub async fn set_caged_in(
    conn: &mut SqliteConnection,
    id: &str,
    caged: bool,
) -> Result<(), String> {
    ensure_pair(conn, id).await?;

    normalize(conn, id).await?;

    if caged {
        sqlx::query(
            r#"UPDATE pet_activity 
            SET is_activity=0
                ,updated_at=CURRENT_TIMESTAMP 
            WHERE pet_id=?1 
                AND owner_id='local-owner' 
                AND is_activity=1"#,
        )
        .bind(id)
        .execute(conn)
        .await
        .map_err(|_| "無法停止寵物活動".to_owned())?;
    } else {
        // Preserve sitting/other active modes on repeated open; only caged pets start walking.
        let active: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(
                SELECT 1 
                FROM pet_activity 
                WHERE pet_id=?1 
                AND owner_id='local-owner' 
                AND is_activity=1)"#,
        )
        .bind(id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| "無法讀取活動".to_owned())?;

        if !active {
            sqlx::query(
                r#"UPDATE pet_activity 
                SET is_activity=1
                    ,updated_at=CURRENT_TIMESTAMP 
                WHERE pet_id=?1 
                    AND owner_id='local-owner' 
                    AND activity_type=?2"#,
            )
            .bind(id)
            .bind(WALK)
            .execute(conn)
            .await
            .map_err(|_| "無法開始散步".to_owned())?;
        }
    }
    Ok(())
}

pub async fn set_caged(pool: &SqlitePool, id: &str, caged: bool) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "無法開始活動交易".to_owned())?;

    set_caged_in(&mut tx, id, caged).await?;

    tx.commit().await.map_err(|_| "無法提交活動".to_owned())
}

pub async fn switch(pool: &SqlitePool, id: &str, walking: bool) -> Result<ActivityStatus, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "無法開始活動交易".to_owned())?;

    ensure_pair(&mut tx, id).await?;

    sqlx::query(
        r#"UPDATE pet_activity 
        SET is_activity=(activity_type=?2)
            ,updated_at=CURRENT_TIMESTAMP 
        WHERE pet_id=?1 
            AND owner_id='local-owner' 
            AND activity_type IN (?3,?4)"#,
    )
    .bind(id)
    .bind(if walking { WALK } else { SIT })
    .bind(WALK)
    .bind(SIT)
    .execute(&mut *tx)
    .await
    .map_err(|_| "無法切換活動".to_owned())?;

    let state = status_in(&mut tx, id).await?;

    tx.commit()
        .await
        .map_err(|_| "無法提交活動切換".to_owned())?;

    Ok(state)
}

pub async fn set_default_caged(
    pool: &SqlitePool,
    id: &str,
    caged: bool,
) -> Result<ActivityStatus, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "無法開始預設資格交易".to_owned())?;

    local(&mut tx, id).await?;

    sqlx::query(
        r#"UPDATE pet_profile 
        SET is_default_closure=?1
            ,updated_at=CURRENT_TIMESTAMP 
        WHERE id=?2 
            AND owner_id='local-owner'"#,
    )
    .bind(caged)
    .bind(id)
    .execute(&mut *tx)
    .await
    .map_err(|_| "無法保存預設資格".to_owned())?;

    let state = status_in(&mut tx, id).await?;

    tx.commit()
        .await
        .map_err(|_| "預設資格提交失敗".to_owned())?;

    Ok(state)
}

pub async fn prepare_startup(pool: &SqlitePool) -> Result<Vec<(String, String, String)>, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "無法開始啟動交易".to_owned())?;

    let invalid: i64 = sqlx::query_scalar(
        r#"SELECT count(*) 
        FROM pet_profile 
        WHERE owner_id='local-owner' 
            AND COALESCE(is_default_closure,0) NOT IN (0,1)"#,
    )
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "無法讀取預設資格".to_owned())?;

    if invalid != 0 {
        return Err("預設資格資料不合法".into());
    }

    sqlx::query(
        r#"UPDATE pet_activity 
        SET is_activity=0
            ,updated_at=CURRENT_TIMESTAMP 
        WHERE owner_id='local-owner' 
            AND is_activity=1 
            AND EXISTS(
                SELECT 1 
                FROM pet_profile p 
                WHERE p.id=pet_activity.pet_id 
                    AND p.owner_id=pet_activity.owner_id 
                    AND p.is_default_closure=1
            )"#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| "無法準備啟動活動".to_owned())?;

    let rows = sqlx::query_as(
        r#"SELECT id
            ,name
            ,species 
        FROM pet_profile 
        WHERE owner_id='local-owner' 
            AND COALESCE(is_default_closure,0)=0 
        ORDER BY name
            ,id"#,
    )
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| "無法讀取啟動寵物".to_owned())?;

    tx.commit()
        .await
        .map_err(|_| "啟動活動提交失敗".to_owned())?;

    Ok(rows)
}

pub async fn snapshot(pool: &SqlitePool, id: &str) -> Result<ActivitySnapshot, String> {
    // Resolve sit priority before capturing a compensating state.
    status(pool, id).await?;

    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "無法讀取活動快照".to_owned())?;

    ensure_pair(&mut tx, id).await?;

    let values = sqlx::query_as(
        r#"SELECT id
            ,is_activity 
        FROM pet_activity 
        WHERE pet_id=?1 
            AND owner_id='local-owner' 
        ORDER BY id"#,
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| "無法讀取活動快照".to_owned())?;

    let selected = sqlx::query_scalar(
        r#"SELECT pet_id 
        FROM local_pet_selection 
        WHERE slot=1"#,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "無法讀取選取快照".to_owned())?;

    tx.commit().await.map_err(|_| "快照提交失敗".to_owned())?;
    Ok(ActivitySnapshot { values, selected })
}

pub async fn restore(
    pool: &SqlitePool,
    id: &str,
    snapshot: &ActivitySnapshot,
) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "無法開始活動補償".to_owned())?;

    local(&mut tx, id).await?;

    for (activity, value) in &snapshot.values {
        let result = sqlx::query(
            r#"UPDATE pet_activity 
            SET is_activity=?1 
            WHERE id=?2 
                AND pet_id=?3 
                AND owner_id='local-owner'"#,
        )
        .bind(value)
        .bind(activity)
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|_| "活動補償失敗".to_owned())?;

        if result.rows_affected() != 1 {
            return Err("活動快照已變更，無法補償".into());
        }
    }

    sqlx::query(
        r#"DELETE FROM local_pet_selection 
        WHERE slot=1"#,
    )
    .execute(&mut *tx)
    .await
    .map_err(|_| "選取補償失敗".to_owned())?;

    if let Some(selected) = &snapshot.selected {
        sqlx::query(r#"INSERT INTO local_pet_selection(slot,pet_id) VALUES(1,?1)"#)
            .bind(selected)
            .execute(&mut *tx)
            .await
            .map_err(|_| "選取補償失敗".to_owned())?;
    }

    tx.commit().await.map_err(|_| "活動補償提交失敗".to_owned())
}
