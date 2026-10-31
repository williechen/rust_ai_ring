-- Members have actions only: definitions never grant access by themselves.
INSERT INTO mbr_perm(id,perm_name,perm_status,created_at,updated_at) VALUES
('member-room-create','建立聊天室','active',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP),
('member-profile-update','修改資料','active',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP),
('member-password-update','修改密碼','active',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP);
-- Explicit default role preserves existing create-room capability and provides required self-service.
INSERT INTO mbr_role(id,role_name,role_status,created_at,updated_at)
VALUES('member-default','一般會員','active',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP);
INSERT INTO mbr_role_perm(role_id,perm_id,created_at)
SELECT 'member-default',id,CURRENT_TIMESTAMP FROM mbr_perm
WHERE id IN('member-room-create','member-profile-update','member-password-update');
INSERT INTO mbr_user_role(user_id,role_id,created_at)
SELECT id,'member-default',CURRENT_TIMESTAMP FROM mbr_user WHERE user_status='active';