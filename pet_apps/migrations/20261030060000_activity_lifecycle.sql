-- Data-only forward migration: no columns, no deletion, no edits to deployed checksums.
-- For legacy profiles with no activities, preserve the old outside state once as walking.
INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity,is_use_ai)
SELECT 'initial-walk:' || p.id,p.id,p.owner_id,'walk',
       CASE WHEN COALESCE(p.is_default_closure,0)=0
                 AND NOT EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id=p.id AND a.owner_id=p.owner_id)
            THEN 1 ELSE 0 END,0
FROM pet_profile p
WHERE NOT EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id=p.id AND a.owner_id=p.owner_id AND a.activity_type='walk');
INSERT INTO pet_activity(id,pet_id,owner_id,activity_type,is_activity,is_use_ai)
SELECT 'initial-sit:' || p.id,p.id,p.owner_id,'sit',0,0
FROM pet_profile p
WHERE NOT EXISTS(SELECT 1 FROM pet_activity a WHERE a.pet_id=p.id AND a.owner_id=p.owner_id AND a.activity_type='sit');
-- Sitting wins even if there are multiple records for either activity type.
UPDATE pet_activity AS w SET is_activity=0
WHERE w.activity_type='walk' AND w.is_activity=1
AND EXISTS(SELECT 1 FROM pet_activity s WHERE s.pet_id=w.pet_id AND s.owner_id=w.owner_id AND s.activity_type='sit' AND s.is_activity=1);

INSERT INTO codemap(id,category,code,value,description,created_at,updated_at)
VALUES ('initial-walk','activity','walk','散步','Initial walk activity',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP),
('initial-sit','activity','sit','坐下','Initial sit activity',CURRENT_TIMESTAMP,CURRENT_TIMESTAMP);

