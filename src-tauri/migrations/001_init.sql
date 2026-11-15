CREATE TABLE bookmark (
    id          INTEGER PRIMARY KEY,
    url         TEXT    NOT NULL,
    -- 중복 판정용 정규화 URL (스킴/추적 파라미터/끝 슬래시 제거)
    url_norm    TEXT    NOT NULL UNIQUE,
    title       TEXT    NOT NULL DEFAULT '',
    note        TEXT    NOT NULL DEFAULT '',
    rating      INTEGER CHECK (rating BETWEEN 1 AND 5),
    -- manual / extension / chrome / firefox / tor
    source      TEXT    NOT NULL DEFAULT 'manual',
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE INDEX idx_bookmark_created ON bookmark (created_at DESC, id DESC);
CREATE INDEX idx_bookmark_rating  ON bookmark (rating);

CREATE TABLE tag (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);

CREATE TABLE bookmark_tag (
    bookmark_id INTEGER NOT NULL REFERENCES bookmark (id) ON DELETE CASCADE,
    tag_id      INTEGER NOT NULL REFERENCES tag (id)      ON DELETE CASCADE,
    PRIMARY KEY (bookmark_id, tag_id)
) WITHOUT ROWID;

CREATE INDEX idx_bookmark_tag_tag ON bookmark_tag (tag_id);

-- rowid = bookmark.id. trigram 토크나이저라 한글 부분 문자열 검색이 된다 (3글자 이상).
CREATE VIRTUAL TABLE bookmark_fts USING fts5 (title, url, note, tags, tokenize = 'trigram');
