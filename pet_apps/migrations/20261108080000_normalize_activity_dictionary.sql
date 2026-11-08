-- 已知活動名稱由字典提供，既有字典列和自訂名稱均保留。
WITH defaults(id, code, label) AS (
    VALUES ('activity-code:walk', 'walk', '散步'),
           ('activity-code:sit', 'sit', '坐下'),
           ('activity-code:play', 'play', '玩耍'),
           ('activity-code:train', 'train', '訓練')
)
INSERT INTO codemap(id, category, code, value)
SELECT defaults.id, 'activity', defaults.code, defaults.label
FROM defaults
WHERE NOT EXISTS (
    SELECT 1 FROM codemap AS existing
    WHERE existing.category = 'activity'
      AND existing.code = defaults.code AND existing.value = defaults.label
);

-- 名稱只在有唯一有效代碼時轉換。找不到或有多個代碼時，整筆 migration 回滾。
CREATE TEMP TABLE legacy_activity_codes (
    activity_id TEXT PRIMARY KEY,
    code TEXT NOT NULL CONSTRAINT activity_requires_unique_codemap_code CHECK (code <> '')
);

WITH dictionary AS (
    SELECT value, MIN(code) AS code
    FROM codemap
    WHERE category = 'activity'
      AND code GLOB '[A-Za-z]*'
      AND code NOT GLOB '*[^A-Za-z0-9_-]*'
      AND instr(code, char(0)) = 0
    GROUP BY value
    HAVING COUNT(DISTINCT code) = 1
)
INSERT INTO legacy_activity_codes(activity_id, code)
SELECT activity.id, COALESCE(dictionary.code, '')
FROM pet_activity AS activity
LEFT JOIN dictionary ON dictionary.value = activity.activity_type
WHERE activity.activity_type IS NOT NULL
  AND (activity.activity_type NOT GLOB '[A-Za-z]*'
       OR activity.activity_type GLOB '*[^A-Za-z0-9_-]*'
       OR instr(activity.activity_type, char(0)) > 0);

UPDATE pet_activity AS activity
SET activity_type = mapped.code
FROM legacy_activity_codes AS mapped
WHERE activity.id = mapped.activity_id;

-- 已轉換的自訂散步或坐下名稱也可能與舊補資料列重合。
CREATE TEMP TABLE translated_activity_seed_pairs AS
SELECT seed.id AS seed_id, MIN(original.id) AS keep_id
FROM pet_activity AS seed
JOIN pet_activity AS original
    ON original.pet_id = seed.pet_id
   AND original.owner_id = seed.owner_id
   AND original.activity_type = seed.activity_type
   AND original.id <> seed.id
JOIN legacy_activity_codes AS mapped ON mapped.activity_id = original.id
WHERE seed.activity_type IN ('walk', 'sit')
  AND seed.id = 'initial-' || seed.activity_type || ':' || seed.pet_id
GROUP BY seed.id;

UPDATE pet_activity AS original
SET is_activity = CASE WHEN seed.is_activity = 1 THEN 1 ELSE original.is_activity END,
    is_use_ai = CASE WHEN seed.is_use_ai = 1 THEN 1 ELSE original.is_use_ai END
FROM translated_activity_seed_pairs AS pair
JOIN pet_activity AS seed ON seed.id = pair.seed_id
WHERE original.id = pair.keep_id;

DELETE FROM pet_activity
WHERE id IN (SELECT seed_id FROM translated_activity_seed_pairs);

-- 在受轉換影響的寵物上保留坐下優先規則。
UPDATE pet_activity AS walk
SET is_activity = 0
WHERE walk.activity_type = 'walk' AND walk.is_activity = 1
  AND EXISTS (
    SELECT 1 FROM legacy_activity_codes AS mapped
    JOIN pet_activity AS activity ON activity.id = mapped.activity_id
    WHERE mapped.code IN ('walk', 'sit')
      AND activity.pet_id = walk.pet_id AND activity.owner_id = walk.owner_id
  )
  AND EXISTS (
    SELECT 1 FROM pet_activity AS sit
    WHERE sit.pet_id = walk.pet_id AND sit.owner_id = walk.owner_id
      AND sit.activity_type = 'sit' AND sit.is_activity = 1
  );

DROP TABLE translated_activity_seed_pairs;
DROP TABLE legacy_activity_codes;

-- NULL 仍表示未指定；所有非空值必須是英文代碼。
CREATE TRIGGER pet_activity_code_on_insert
BEFORE INSERT ON pet_activity
WHEN NEW.activity_type IS NOT NULL
  AND (NEW.activity_type NOT GLOB '[A-Za-z]*'
       OR NEW.activity_type GLOB '*[^A-Za-z0-9_-]*'
       OR instr(NEW.activity_type, char(0)) > 0)
BEGIN
    SELECT RAISE(ABORT, 'pet_activity.activity_type 須使用英文代碼');
END;

CREATE TRIGGER pet_activity_code_on_update
BEFORE UPDATE OF activity_type ON pet_activity
WHEN NEW.activity_type IS NOT NULL
  AND (NEW.activity_type NOT GLOB '[A-Za-z]*'
       OR NEW.activity_type GLOB '*[^A-Za-z0-9_-]*'
       OR instr(NEW.activity_type, char(0)) > 0)
BEGIN
    SELECT RAISE(ABORT, 'pet_activity.activity_type 須使用英文代碼');
END;