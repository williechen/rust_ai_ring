-- Add definitions and explicit admin page/CRUD grants without changing the f6 migration checksum.
INSERT INTO uac_acl(id,acl_name,acl_path,acl_category,acl_parent_id,acl_status,created_at,updated_at) VALUES
('analytics-roles','角色管理','/roles/index','page','analytics-members','active',now(),now()),
('analytics-acls','頁面管理','/acls/index','page','analytics-members','active',now(),now())
ON CONFLICT (id) DO NOTHING;
INSERT INTO uac_acl(id,acl_name,acl_path,acl_category,acl_parent_id,acl_status,created_at,updated_at)
SELECT p.id||'-'||op,op,op,'action',p.id,'active',now(),now()
FROM uac_acl p CROSS JOIN (VALUES('create'),('read'),('update'),('delete')) AS crud(op)
WHERE p.id IN('analytics-roles','analytics-acls') ON CONFLICT(id) DO NOTHING;
INSERT INTO uac_role_acl(role_id,acl_id,created_at)
SELECT 'analytics-admin',a.id,now() FROM uac_acl a
WHERE a.id IN('analytics-roles','analytics-acls') OR a.acl_parent_id IN('analytics-roles','analytics-acls')
ON CONFLICT(role_id,acl_id) DO NOTHING;