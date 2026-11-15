-- 앱에서 북마크를 연 횟수와 마지막으로 연 시각 (unix 초). 얼마나 자주 쓰는지 보는 용도.
ALTER TABLE bookmark ADD COLUMN open_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE bookmark ADD COLUMN last_opened_at INTEGER;

CREATE INDEX idx_bookmark_open_count ON bookmark (open_count DESC);
