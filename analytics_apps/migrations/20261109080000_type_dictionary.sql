CREATE TABLE codemap (
    category TEXT NOT NULL,
    code TEXT NOT NULL CHECK (code ~ '^[A-Za-z][A-Za-z0-9._-]{0,127}$'),
    value TEXT NOT NULL CHECK (btrim(value) <> ''),
    wire_value TEXT COLLATE "C",
    PRIMARY KEY (category, code),
    UNIQUE (category, wire_value),
    CHECK (
        (category = 'event_type' AND wire_value IS NOT NULL AND wire_value <> ''
         AND char_length(wire_value) <= 128 AND wire_value = btrim(wire_value))
        OR (category <> 'event_type' AND wire_value IS NULL)
    )
);

-- 相容既有上報協定，包含混合文字。預留前綴不與相同外形的協定值共用 key。
CREATE FUNCTION analytics_event_key(wire TEXT) RETURNS TEXT
LANGUAGE SQL IMMUTABLE STRICT
RETURN CASE
    WHEN wire ~ '^[A-Za-z][A-Za-z0-9._-]{0,127}$' AND wire NOT LIKE 'wire-sha256-%' THEN wire
    ELSE 'wire-sha256-' || encode(sha256(convert_to(wire, 'UTF8')), 'hex')
END;

INSERT INTO codemap(category, code, value, wire_value) VALUES
    ('source_app', 'web_chat', '網頁聊天室', NULL),
    ('source_app', 'desktop_pet', '桌面寵物', NULL),
    ('event_type', 'chat.message.persisted', '聊天室訊息已儲存', 'chat.message.persisted'),
    ('event_type', 'pet.sleep', '寵物進入睡眠', 'pet.sleep');

-- 不覆寫內建中文名稱。原協定值採精確映射，碰撞由主鍵攔下並回滾。
INSERT INTO codemap(category, code, value, wire_value)
SELECT DISTINCT 'event_type', analytics_event_key(event_type), event_type, event_type
FROM product_events
ON CONFLICT (category, wire_value) DO NOTHING;

UPDATE product_events AS event
SET event_type = dictionary.code
FROM codemap AS dictionary
WHERE dictionary.category = 'event_type'
  AND dictionary.wire_value = event.event_type
  AND event.event_type <> dictionary.code;

ALTER TABLE product_events
    ADD COLUMN source_namespace TEXT GENERATED ALWAYS AS ('source_app'::TEXT) STORED,
    ADD COLUMN event_namespace TEXT GENERATED ALWAYS AS ('event_type'::TEXT) STORED,
    ADD CONSTRAINT product_events_source_key FOREIGN KEY (source_namespace, source_app)
        REFERENCES codemap(category, code) ON UPDATE RESTRICT ON DELETE RESTRICT,
    ADD CONSTRAINT product_events_type_key FOREIGN KEY (event_namespace, event_type)
        REFERENCES codemap(category, code) ON UPDATE RESTRICT ON DELETE RESTRICT;

CREATE INDEX product_events_by_source_and_type ON product_events(source_app, event_type);
CREATE INDEX product_events_by_event_type ON product_events(event_type);

CREATE FUNCTION analytics_resolve_event_key() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
DECLARE
    requested TEXT := NEW.event_type;
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF NEW.event_type = OLD.event_type THEN
            RETURN NEW;
        END IF;
    END IF;
    INSERT INTO codemap(category, code, value, wire_value)
    VALUES ('event_type', analytics_event_key(requested), requested, requested)
    ON CONFLICT (category, wire_value) DO NOTHING;
    SELECT code INTO STRICT NEW.event_type
    FROM codemap WHERE category = 'event_type' AND wire_value = requested;
    RETURN NEW;
END;
$$;

CREATE TRIGGER product_events_resolve_dictionary_key
BEFORE INSERT OR UPDATE OF event_type ON product_events
FOR EACH ROW EXECUTE FUNCTION analytics_resolve_event_key();

-- 更新中文名稱不影響歷史協定。內建來源 key 即使尚無事件也需保持可用。
CREATE FUNCTION analytics_protect_dictionary_identity() RETURNS TRIGGER
LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        IF OLD.category = 'source_app' AND OLD.code IN ('web_chat', 'desktop_pet') THEN
            RAISE EXCEPTION '內建來源代碼不可移除';
        END IF;
        RETURN OLD;
    END IF;
    IF OLD.category = 'source_app' AND OLD.code IN ('web_chat', 'desktop_pet')
        AND (NEW.category, NEW.code) IS DISTINCT FROM (OLD.category, OLD.code) THEN
        RAISE EXCEPTION '內建來源代碼不可更改';
    END IF;
    IF OLD.category = 'event_type' AND NEW.wire_value IS DISTINCT FROM OLD.wire_value THEN
        RAISE EXCEPTION '事件的協定值不可更改';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER codemap_preserve_protocol_identity
BEFORE UPDATE OF category, code, wire_value OR DELETE ON codemap
FOR EACH ROW EXECUTE FUNCTION analytics_protect_dictionary_identity();