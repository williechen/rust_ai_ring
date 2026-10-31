-- Additive access policy using the existing UAC role/ACL tables.
CREATE UNIQUE INDEX uq_uac_account_day45 ON uac_user(lower(trim(user_acnt)));
INSERT INTO uac_role(id,role_name,role_status,created_at,updated_at) VALUES
('analytics-admin','system_admin','active',now(),now()),
('analytics-viewer','viewer','active',now(),now());
-- Exactly three levels. Existing acl_category and acl_parent_id are reused.
INSERT INTO uac_acl(id,acl_name,acl_path,acl_category,acl_parent_id,acl_status,created_at,updated_at) VALUES
('analytics-members','會員管理','','category',NULL,'active',now(),now()),
('analytics-info','資訊分析','','category',NULL,'active',now(),now()),
('analytics-token','認證管理','','category',NULL,'active',now(),now());
INSERT INTO uac_acl(id,acl_name,acl_path,acl_category,acl_parent_id,acl_status,created_at,updated_at) VALUES
('analytics-accounts','帳號管理','/accounts/new','page','analytics-members','active',now(),now()),
('analytics-roles','角色管理','/roles/index','page','analytics-members','active',now(),now()),
('analytics-acls','頁面管理','/acls/index','page','analytics-members','active',now(),now()),
('analytics-dashboard','分析後台','/dashboard','page','analytics-info','active',now(),now()),
('analytics-chat','Chat來源','/api/v1/events/web_chat','page','analytics-info','active',now(),now()),
('analytics-pet','Pet來源','/api/v1/events/desktop_pet','page','analytics-info','active',now(),now()),
('analytics-summary','彙總查詢','/api/v1/events/{source}/summary','page','analytics-info','active',now(),now()),
('analytics-kind','類型查詢','/api/v1/events/{source}/summary/{event_type}','page','analytics-info','active',now(),now()),
('analytics-export','CSV匯出','/api/v1/events/{source}/export.csv','page','analytics-info','active',now(),now()),
('analytics-password','自身密碼','/change-password','page','analytics-members','active',now(),now()),
('analytics-security','管理憑證','/security','page','analytics-token','active',now(),now());
INSERT INTO uac_acl(id,acl_name,acl_path,acl_category,acl_parent_id,acl_status,created_at,updated_at)
SELECT p.id || '-' || operation, operation, operation, 'action', p.id, 'active', now(),now()
FROM uac_acl p CROSS JOIN (VALUES('create'),('read'),('update'),('delete')) AS crud(operation)
WHERE p.id IN ('analytics-accounts','analytics-dashboard','analytics-chat','analytics-pet','analytics-summary','analytics-kind','analytics-export','analytics-password','analytics-security', 'analytics-roles', 'analytics-acls');
INSERT INTO uac_user_role(user_id,role_id,created_at)
SELECT id,'analytics-admin',now() FROM uac_user WHERE lower(user_acnt)='admin';
CREATE TABLE analytics_account_audit (
 id TEXT PRIMARY KEY, actor_id TEXT NOT NULL, target_id TEXT NOT NULL,
 action TEXT NOT NULL, result TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
-- Admin default: all seeded pages and their CRUD actions (no implicit category grant).
INSERT INTO uac_role_acl(role_id,acl_id,created_at)
SELECT 'analytics-admin',a.id,now() FROM uac_acl a
WHERE a.id IN ('analytics-accounts','analytics-dashboard','analytics-chat','analytics-pet','analytics-summary','analytics-kind','analytics-export','analytics-password','analytics-security', 'analytics-roles', 'analytics-acls')
 OR a.acl_parent_id IN ('analytics-accounts','analytics-dashboard','analytics-chat','analytics-pet','analytics-summary','analytics-kind','analytics-export','analytics-password','analytics-security', 'analytics-roles', 'analytics-acls');
-- Viewer default: read operations; source page visibility is independently granted per user.
INSERT INTO uac_role_acl(role_id,acl_id,created_at)
SELECT 'analytics-viewer',a.id,now() FROM uac_acl a
WHERE a.id IN ('analytics-dashboard','analytics-password','analytics-summary','analytics-kind','analytics-export')
 OR (a.acl_category='action' AND a.acl_path='read' AND a.acl_parent_id IN ('analytics-dashboard','analytics-chat','analytics-pet','analytics-summary','analytics-kind','analytics-export','analytics-password'));