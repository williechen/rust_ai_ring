/// 有效page與action皆須授權，個人deny優先。
pub const PERMISSION_SQL: &str = r#"
WITH node AS (
    SELECT p.id AS page_id
        ,c.id AS category_id
        ,a.id AS action_id
    FROM uac_acl p 
    JOIN uac_acl c 
        ON c.id=p.acl_parent_id
    JOIN uac_acl a 
        ON a.acl_parent_id=p.id
    WHERE p.acl_category='page' 
        AND p.acl_path=$2 
        AND p.acl_status='active'
        AND c.acl_category='category' 
        AND c.acl_parent_id IS NULL 
        AND c.acl_status='active'
        AND a.acl_category='action' 
        AND a.acl_path=$3 
        AND a.acl_status='active'
        AND (
            SELECT count(*) 
            FROM uac_acl p2 
            WHERE p2.acl_category='page' 
                AND p2.acl_path=$2
        )=1
        AND (
            SELECT count(*) 
            FROM uac_acl a2 
            WHERE a2.acl_parent_id=p.id 
                AND a2.acl_category='action' 
                AND a2.acl_path=$3
        )=1
), 
person AS (
    SELECT s.user_id 
    FROM uac_login_session s 
    JOIN uac_user u 
        ON u.id=s.user_id
    WHERE s.token_hash=$1 
        AND s.expires_at>floor(extract(epoch from now()))::bigint
        AND u.user_status='active'
)
        
SELECT EXISTS(
    SELECT 1 
    FROM node n 
    CROSS JOIN person p
    WHERE NOT EXISTS(
        SELECT 1 
        FROM uac_user_acl ua 
        WHERE ua.user_id=p.user_id
        AND ua.acl_id IN(n.category_id,n.page_id,n.action_id) 
        AND ua.acl_filter='deny'
    )
    AND NOT EXISTS(
        SELECT 1 
        FROM (VALUES(n.page_id),(n.action_id)) AS needed(id)
        WHERE NOT EXISTS(
            SELECT 1 
            FROM uac_user_role ur 
            JOIN uac_role r 
                ON r.id=ur.role_id
            JOIN uac_role_acl ra 
                ON ra.role_id=r.id 
                AND ra.acl_id=needed.id
            WHERE ur.user_id=p.user_id 
                AND r.role_status='active'
        )
            AND NOT EXISTS(
                SELECT 1 
                FROM uac_user_acl ua 
                WHERE ua.user_id=p.user_id
                    AND ua.acl_id=needed.id 
                    AND ua.acl_filter='allow'
            )
    )
)
"#;
