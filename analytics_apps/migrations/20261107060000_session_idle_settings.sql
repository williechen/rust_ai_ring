-- 遞增 migration：既有檔案與校驗碼不得修改。
CREATE TABLE analytics_session_policy (
    id SMALLINT PRIMARY KEY CHECK(id=1),
    idle_seconds BIGINT NOT NULL CHECK(idle_seconds BETWEEN 60 AND 86400),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO analytics_session_policy(id,idle_seconds) VALUES(1,1800);
-- 沿原認證管理分類，只有管理員具有明確的 CRUD 授權。
INSERT INTO uac_acl(id,acl_name,acl_path,acl_category,acl_parent_id,acl_status,created_at,updated_at)
VALUES('analytics-session','工作階段設定','/settings/session','page','analytics-token','active',now(),now());
INSERT INTO uac_acl(id,acl_name,acl_path,acl_category,acl_parent_id,acl_status,created_at,updated_at)
SELECT 'analytics-session-'||op,op,op,'action','analytics-session','active',now(),now()
FROM (VALUES('create'),('read'),('update'),('delete')) AS crud(op);
INSERT INTO uac_role_acl(role_id,acl_id,created_at)
SELECT 'analytics-admin',id,now() FROM uac_acl
WHERE id='analytics-session' OR acl_parent_id='analytics-session';