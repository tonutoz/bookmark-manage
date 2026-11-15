-- 사이트별 묶음용 도메인. www. 같은 접두어를 뗀 호스트 이름이고, http(s) 가 아닌 주소는 빈 문자열이다.
-- 기존 행은 마이그레이션 직후 코드에서 채운다 (bookmarks::backfill_hosts).
ALTER TABLE bookmark ADD COLUMN host TEXT NOT NULL DEFAULT '';

CREATE INDEX idx_bookmark_host ON bookmark (host);
