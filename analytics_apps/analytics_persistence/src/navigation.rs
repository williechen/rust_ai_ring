/// 查詢已授權頁面；動作欄位只承接既有資料契約，不決定一般導覽呈現。
const PAGE_SQL: &str = r#"
WITH person AS (
    SELECT s.user_id 
    FROM uac_login_session s 
    JOIN uac_user u 
        ON u.id=s.user_id
    WHERE s.token_hash=$1 
        AND s.expires_at>floor(extract(epoch from now()))::bigint 
        AND u.user_status='active'
), 
granted AS (
    SELECT a.* 
    FROM uac_acl a 
    CROSS JOIN person p 
    WHERE a.acl_status='active'
        AND NOT EXISTS(
            SELECT 1 
            FROM uac_user_acl x 
            WHERE x.user_id=p.user_id 
                AND x.acl_id=a.id 
                AND x.acl_filter='deny'
        )
        AND (
            EXISTS(
                SELECT 1 
                FROM uac_user_acl x 
                WHERE x.user_id=p.user_id 
                    AND x.acl_id=a.id 
                    AND x.acl_filter='allow'
            )
            OR EXISTS(
                SELECT 1 
                FROM uac_user_role ur 
                JOIN uac_role r ON r.id=ur.role_id 
                JOIN uac_role_acl ra ON ra.role_id=r.id
                WHERE ur.user_id=p.user_id 
                AND r.role_status='active' 
                AND ra.acl_id=a.id
            )
        )
)

SELECT c.id
    ,c.acl_name
    ,p.id
    ,p.acl_name
    ,p.acl_path
    ,a.id
    ,a.acl_name
FROM granted p 
JOIN uac_acl c 
    ON c.id=p.acl_parent_id 
    AND c.acl_category='category'
    AND c.acl_parent_id IS NULL 
    AND c.acl_status='active'
CROSS JOIN person who
LEFT JOIN granted a 
    ON a.acl_category='action' 
    AND a.acl_parent_id=p.id
    AND a.acl_path IN ('create','read','update','delete')
    AND (
        SELECT count(*) 
        FROM uac_acl other 
        WHERE other.acl_parent_id=p.id 
           AND other.acl_category='action' 
           AND other.acl_path=a.acl_path
    )=1
WHERE p.acl_category='page'
    AND (SELECT count(*) 
         FROM uac_acl other 
         WHERE other.acl_category='page' 
           AND other.acl_path=p.acl_path)=1
    AND NOT EXISTS(
        SELECT 1 
        FROM uac_user_acl x 
        WHERE x.user_id=who.user_id 
          AND x.acl_id=c.id 
          AND x.acl_filter='deny'
    )
ORDER BY c.id,p.id,a.id
"#;

pub type TreeRow = (
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
);

pub async fn rows(pool: &sqlx::PgPool, token_hash: &str) -> Result<Vec<TreeRow>, sqlx::Error> {
    sqlx::query_as(PAGE_SQL)
        .bind(token_hash)
        .fetch_all(pool)
        .await
}
