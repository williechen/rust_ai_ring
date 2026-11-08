-- 對應同一寵物、主人和代碼，只合併舊補資料 migration 產生的列。
CREATE TEMP TABLE generated_activity_pairs AS
SELECT seed.id AS seed_id, MIN(original.id) AS keep_id
FROM pet_activity AS seed
JOIN pet_activity AS original
    ON original.pet_id = seed.pet_id
   AND original.owner_id = seed.owner_id
   AND original.activity_type = seed.activity_type
   AND original.id <> seed.id
WHERE seed.activity_type IN ('walk', 'sit')
  AND seed.id = 'initial-' || seed.activity_type || ':' || seed.pet_id
GROUP BY seed.id;

-- 保留原列的識別與時間，合併補資料列上可能已更新的啟用旗標。
UPDATE pet_activity AS original
SET is_activity = CASE WHEN seed.is_activity = 1 THEN 1 ELSE original.is_activity END,
    is_use_ai = CASE WHEN seed.is_use_ai = 1 THEN 1 ELSE original.is_use_ai END
FROM generated_activity_pairs AS pair
JOIN pet_activity AS seed ON seed.id = pair.seed_id
WHERE original.id = pair.keep_id;

-- 同一組活動合併後若同時啟用，坐下優先，其他寵物不受影響。
UPDATE pet_activity AS walk
SET is_activity = 0
WHERE walk.activity_type = 'walk'
  AND walk.is_activity = 1
  AND EXISTS (
    SELECT 1 FROM generated_activity_pairs AS pair
    JOIN pet_activity AS seed ON seed.id = pair.seed_id
    WHERE seed.pet_id = walk.pet_id AND seed.owner_id = walk.owner_id
  )
  AND EXISTS (
    SELECT 1 FROM pet_activity AS sit
    WHERE sit.pet_id = walk.pet_id AND sit.owner_id = walk.owner_id
      AND sit.activity_type = 'sit' AND sit.is_activity = 1
  );

DELETE FROM pet_activity
WHERE id IN (SELECT seed_id FROM generated_activity_pairs);

DROP TABLE generated_activity_pairs;