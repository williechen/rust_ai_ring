-- Preserve adopted role defaults. Required initial change is provisioned per person.
-- Never overwrite an explicit personal deny; administrators must resolve that conflict.
INSERT INTO uac_user_acl(user_id,acl_id,acl_filter,created_at)
SELECT u.id,a.id,'allow',now() FROM uac_user u
CROSS JOIN (VALUES('analytics-password'),('analytics-password-update')) a(id)
WHERE u.is_init_pd=TRUE AND u.user_status='active'
AND NOT EXISTS(SELECT 1 FROM uac_user_acl x WHERE x.user_id=u.id AND x.acl_id=a.id)
AND NOT EXISTS(SELECT 1 FROM uac_user_role ur JOIN uac_role r ON r.id=ur.role_id
 JOIN uac_role_acl ra ON ra.role_id=r.id WHERE ur.user_id=u.id
 AND r.role_status='active' AND ra.acl_id=a.id);