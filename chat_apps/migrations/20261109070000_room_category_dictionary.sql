CREATE TABLE codemap (
    category TEXT NOT NULL,
    code TEXT NOT NULL CHECK (
        code GLOB '[A-Za-z]*' AND code NOT GLOB '*[^A-Za-z0-9_-]' AND instr(code, char(0)) = 0
    ),
    value TEXT NOT NULL CHECK (length(trim(value)) > 0),
    PRIMARY KEY (category, code)
);

INSERT INTO codemap (category, code, value) VALUES
    ('room_category', 'default-category', '一般聊天室'),
    ('room_category', 'category-programming', '程式設計');

-- 只轉換唯一匹配的舊名稱，既有合法 key 保留原值。
WITH labels AS (
    SELECT value, min(code) AS code FROM codemap
    WHERE category = 'room_category'
    GROUP BY value HAVING count(*) = 1
)
UPDATE rooms
SET category_room_id = labels.code
FROM labels
WHERE rooms.category_room_id = labels.value
  AND NOT EXISTS (
      SELECT 1 FROM codemap
      WHERE category = 'room_category' AND code = rooms.category_room_id
  );

-- 留有未知或歧義的值時，整筆 migration 回滾。
CREATE TEMP TABLE room_category_validation (
    invalid INTEGER NOT NULL CONSTRAINT room_category_requires_codemap_key CHECK (invalid = 0)
);
INSERT INTO room_category_validation
SELECT EXISTS (
    SELECT 1 FROM rooms r
    WHERE NOT EXISTS (
        SELECT 1 FROM codemap c WHERE c.category = 'room_category' AND c.code = r.category_room_id
    )
);
DROP TABLE room_category_validation;

CREATE TRIGGER room_category_insert_guard
BEFORE INSERT ON rooms
WHEN NOT EXISTS (
    SELECT 1 FROM codemap WHERE category = 'room_category' AND code = NEW.category_room_id
)
BEGIN
    SELECT RAISE(ABORT, '房間種類未登錄');
END;

CREATE TRIGGER room_category_update_guard
BEFORE UPDATE OF category_room_id ON rooms
WHEN NOT EXISTS (
    SELECT 1 FROM codemap WHERE category = 'room_category' AND code = NEW.category_room_id
)
BEGIN
   SELECT RAISE(ABORT, '房間種類未登錄');
END;

CREATE TRIGGER room_category_dictionary_delete_guard
BEFORE DELETE ON codemap
WHEN OLD.category = 'room_category'
  AND EXISTS (SELECT 1 FROM rooms WHERE category_room_id = OLD.code)
BEGIN
    SELECT RAISE(ABORT, '房間仍引用此種類');
END;

CREATE TRIGGER room_category_dictionary_update_guard
BEFORE UPDATE OF category, code ON codemap
WHEN OLD.category = 'room_category'
  AND (NEW.category <> OLD.category OR NEW.code <> OLD.code)
  AND EXISTS (SELECT 1 FROM rooms WHERE category_room_id = OLD.code)
BEGIN
    SELECT RAISE(ABORT, '房間仍引用此種類');
END;