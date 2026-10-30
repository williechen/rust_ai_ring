-- Preserve all activity IDs, owner links, dictionary rows and deployed migration checksums.
UPDATE pet_activity SET activity_type=CASE activity_type WHEN '散步' THEN 'walk' WHEN '坐下' THEN 'sit' END
WHERE activity_type IN ('散步','坐下');
UPDATE pet_activity AS w SET is_activity=0
WHERE w.activity_type='walk' AND w.is_activity=1
AND EXISTS(SELECT 1 FROM pet_activity s WHERE s.pet_id=w.pet_id AND s.owner_id=w.owner_id AND s.activity_type='sit' AND s.is_activity=1);