-- 舊事件發生時間未知，保留NULL而不以接收時間回填。
ALTER TABLE product_events ADD COLUMN occurred_at TIMESTAMPTZ CHECK (occurred_at IS NULL OR isfinite(occurred_at));
CREATE INDEX product_events_occurrence_by_source ON product_events(source_app,occurred_at) WHERE occurred_at IS NOT NULL;