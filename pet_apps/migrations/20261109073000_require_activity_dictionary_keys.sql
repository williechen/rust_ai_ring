-- 升級前先確認既有代碼，包括新寵物固定使用的兩個活動。
CREATE INDEX codemap_activity_lookup ON codemap(category, code);
CREATE INDEX pet_activity_type_lookup ON pet_activity(activity_type);

CREATE TEMP TABLE missing_activity_dictionary (
    code TEXT CONSTRAINT pet_activity_requires_codemap_key CHECK (code IS NULL)
);
INSERT INTO missing_activity_dictionary(code)
SELECT required.code
FROM (
    SELECT activity_type AS code FROM pet_activity WHERE activity_type IS NOT NULL
    UNION SELECT 'walk'
    UNION SELECT 'sit'
) AS required
WHERE NOT EXISTS (
    SELECT 1 FROM codemap
    WHERE category = 'activity' AND code = required.code
);
DROP TABLE missing_activity_dictionary;

CREATE TRIGGER require_activity_dictionary_insert
BEFORE INSERT ON pet_activity
WHEN NEW.activity_type IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM codemap WHERE category = 'activity' AND code = NEW.activity_type
)
BEGIN
    SELECT RAISE(ABORT, '活動代碼尚未登錄至 codemap');
END;

CREATE TRIGGER require_activity_dictionary_update
BEFORE UPDATE OF activity_type ON pet_activity
WHEN NEW.activity_type IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM codemap WHERE category = 'activity' AND code = NEW.activity_type
)
BEGIN
    SELECT RAISE(ABORT, '活動代碼尚未登錄至 codemap');
END;

-- 仍有同代碼的其他字典列時，可移除別名。最後一列保護資料及固定活動。
CREATE VIEW required_activity_dictionary_rows AS
SELECT entry.id
FROM codemap AS entry
WHERE entry.category = 'activity'
  AND (
      entry.code IN ('walk', 'sit')
      OR EXISTS (SELECT 1 FROM pet_activity WHERE activity_type = entry.code)
  )
  AND NOT EXISTS (
      SELECT 1 FROM codemap AS alias
      WHERE alias.category = entry.category AND alias.code = entry.code AND alias.id <> entry.id
  );

CREATE TRIGGER keep_activity_dictionary_delete
BEFORE DELETE ON codemap
WHEN EXISTS (SELECT 1 FROM required_activity_dictionary_rows WHERE id = OLD.id)
BEGIN
    SELECT RAISE(ABORT, '活動仍使用此字典代碼');
END;

CREATE TRIGGER keep_activity_dictionary_update
BEFORE UPDATE OF id, category, code ON codemap
WHEN (
    (NEW.category IS NOT OLD.category OR NEW.code IS NOT OLD.code)
    AND EXISTS (SELECT 1 FROM required_activity_dictionary_rows WHERE id = OLD.id)
) OR (
    NEW.id IS NOT OLD.id
    AND EXISTS (SELECT 1 FROM required_activity_dictionary_rows WHERE id = NEW.id)
)
BEGIN
    SELECT RAISE(ABORT, '活動仍使用此字典代碼');
END;

-- SQLite REPLACE 可在遞迴 trigger 關閉時直接替換同主鍵資料列。
CREATE TRIGGER keep_activity_dictionary_replace
BEFORE INSERT ON codemap
WHEN EXISTS (
    SELECT 1
    FROM codemap AS replaced
    JOIN required_activity_dictionary_rows AS required USING (id)
    WHERE replaced.id = NEW.id
      AND (NEW.category IS NOT replaced.category OR NEW.code IS NOT replaced.code)
)
BEGIN
    SELECT RAISE(ABORT, '活動仍使用此字典代碼');
END;