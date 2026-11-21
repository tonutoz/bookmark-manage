use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, params_from_iter, types::Value, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::url_norm::{normalize, site};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub note: String,
    pub rating: Option<i64>,
    pub tags: Vec<String>,
    pub created_at: i64,
    /// 썸네일이 있으면 마지막 변경 시각(ms). 화면에서 캐시 키로 쓴다.
    pub thumb_version: Option<i64>,
    /// 앱에서 이 북마크를 연 횟수
    pub open_count: i64,
    /// 마지막으로 연 시각 (unix 초)
    pub last_opened_at: Option<i64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewBookmark {
    pub url: String,
    pub title: String,
    pub note: String,
    pub rating: Option<i64>,
    pub tags: Vec<String>,
    pub source: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveResult {
    pub bookmark: Bookmark,
    /// true 면 이미 있던 URL이라 새로 만들지 않고 기존 항목을 돌려준 것.
    pub duplicate: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListFilter {
    pub query: Option<String>,
    pub tag: Option<String>,
    pub min_rating: Option<i64>,
    /// 이 사이트(도메인)의 것만
    pub host: Option<String>,
    /// 태그가 하나도 없는 것만
    pub untagged: bool,
    /// 그 사이트(도메인)의 북마크가 이것 하나뿐인 것만
    pub singleton: bool,
    /// 한 번도 열어보지 않은 것만
    pub unopened: bool,
    /// 별점이 없는 것만
    pub unrated: bool,
    /// 썸네일이 없는 것만
    pub no_thumb: bool,
    /// 이 시각(unix 초) 이후로 열어본 적이 없는 것만. 한 번도 안 열었으면 추가한 날을 기준으로 본다.
    pub stale_before: Option<i64>,
    /// 이 시각(unix 초) 이후에 추가된 것만
    pub added_since: Option<i64>,
    /// "recent"(기본) | "oldest" | "rating" | "title" | "opens"
    pub sort: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

const SELECT: &str = "SELECT b.id, b.url, b.title, b.note, b.rating, b.created_at,
        (SELECT group_concat(t.name, char(31))
           FROM bookmark_tag bt JOIN tag t ON t.id = bt.tag_id
          WHERE bt.bookmark_id = b.id),
        (SELECT updated_at FROM thumbs.thumbnail WHERE bookmark_id = b.id),
        b.open_count, b.last_opened_at
   FROM bookmark b";

fn from_row(row: &rusqlite::Row) -> rusqlite::Result<Bookmark> {
    let tags: Option<String> = row.get(6)?;
    Ok(Bookmark {
        id: row.get(0)?,
        url: row.get(1)?,
        title: row.get(2)?,
        note: row.get(3)?,
        rating: row.get(4)?,
        created_at: row.get(5)?,
        thumb_version: row.get(7)?,
        open_count: row.get(8)?,
        last_opened_at: row.get(9)?,
        tags: tags
            .map(|s| s.split('\u{1f}').map(str::to_string).collect())
            .unwrap_or_default(),
    })
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn check_rating(rating: Option<i64>) -> Result<(), String> {
    match rating {
        Some(r) if !(1..=5).contains(&r) => Err("별점은 1~5 사이여야 합니다".into()),
        _ => Ok(()),
    }
}

pub fn get(conn: &Connection, id: i64) -> Result<Bookmark, String> {
    conn.query_row(&format!("{SELECT} WHERE b.id = ?"), [id], from_row)
        .map_err(|e| e.to_string())
}

/// 검색 인덱스의 해당 행을 다시 만든다. 북마크나 그 태그가 바뀔 때마다 호출한다.
fn reindex(conn: &Connection, id: i64) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM bookmark_fts WHERE rowid = ?", [id])?;
    conn.execute(
        "INSERT INTO bookmark_fts (rowid, title, url, note, tags)
         SELECT b.id, b.title, b.url, b.note,
                coalesce((SELECT group_concat(t.name, ' ')
                            FROM bookmark_tag bt JOIN tag t ON t.id = bt.tag_id
                           WHERE bt.bookmark_id = b.id), '')
           FROM bookmark b WHERE b.id = ?",
        [id],
    )?;
    Ok(())
}

fn attach_tags(conn: &Connection, id: i64, tags: &[String]) -> rusqlite::Result<()> {
    for tag in tags {
        let name = tag.trim().trim_start_matches('#').trim();
        if name.is_empty() {
            continue;
        }
        conn.execute("INSERT OR IGNORE INTO tag (name) VALUES (?)", [name])?;
        conn.execute(
            "INSERT OR IGNORE INTO bookmark_tag (bookmark_id, tag_id)
             SELECT ?, id FROM tag WHERE name = ?",
            params![id, name],
        )?;
    }
    Ok(())
}

pub fn add(conn: &Connection, new: NewBookmark) -> Result<SaveResult, String> {
    let url = new.url.trim();
    if url.is_empty() {
        return Err("URL이 비어 있습니다".into());
    }
    check_rating(new.rating)?;
    let url_norm = normalize(url);

    let existing: Option<i64> = conn
        .query_row("SELECT id FROM bookmark WHERE url_norm = ?", [&url_norm], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(id) = existing {
        return Ok(SaveResult { bookmark: get(conn, id)?, duplicate: true });
    }

    let id = (|| -> rusqlite::Result<i64> {
        let tx = conn.unchecked_transaction()?;
        let ts = now();
        tx.execute(
            "INSERT INTO bookmark (url, url_norm, title, note, rating, source, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                url,
                url_norm,
                new.title.trim(),
                new.note,
                new.rating,
                new.source.as_deref().unwrap_or("manual"),
                ts,
                ts
            ],
        )?;
        let id = tx.last_insert_rowid();
        set_host(&tx, id, url)?;
        attach_tags(&tx, id, &new.tags)?;
        reindex(&tx, id)?;
        tx.commit()?;
        Ok(id)
    })()
    .map_err(|e| e.to_string())?;

    Ok(SaveResult { bookmark: get(conn, id)?, duplicate: false })
}

/// 주소에서 뽑은 사이트(도메인)를 기록한다. 주소가 정해지거나 바뀔 때마다 호출한다.
fn set_host(conn: &Connection, id: i64, url: &str) -> rusqlite::Result<()> {
    conn.execute("UPDATE bookmark SET host = ? WHERE id = ?", params![site(url), id])?;
    Ok(())
}

/// host 열이 생기기 전에 저장된 북마크에 사이트를 채운다.
pub(crate) fn backfill_hosts(conn: &Connection) -> rusqlite::Result<()> {
    let rows: Vec<(i64, String)> = conn
        .prepare("SELECT id, url FROM bookmark WHERE host = ''")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let tx = conn.unchecked_transaction()?;
    for (id, url) in rows {
        set_host(&tx, id, &url)?;
    }
    tx.commit()
}

/// 어느 북마크에도 붙어 있지 않은 태그를 지운다.
fn prune_tags(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM tag WHERE id NOT IN (SELECT tag_id FROM bookmark_tag)", [])?;
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BookmarkEdit {
    pub url: String,
    pub title: String,
    pub note: String,
    pub tags: Vec<String>,
}

/// 주소·제목·메모·태그를 바꾼다. 태그는 주어진 목록으로 통째로 교체한다.
pub fn update(conn: &Connection, id: i64, edit: BookmarkEdit) -> Result<Bookmark, String> {
    let url = edit.url.trim();
    if url.is_empty() {
        return Err("URL이 비어 있습니다".into());
    }
    let url_norm = normalize(url);
    let clash: Option<i64> = conn
        .query_row("SELECT id FROM bookmark WHERE url_norm = ? AND id <> ?", params![url_norm, id], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    if clash.is_some() {
        return Err("같은 주소의 북마크가 이미 있습니다".into());
    }
    let changed = (|| -> rusqlite::Result<usize> {
        let tx = conn.unchecked_transaction()?;
        let changed = tx.execute(
            "UPDATE bookmark SET url = ?, url_norm = ?, title = ?, note = ?, updated_at = ? WHERE id = ?",
            params![url, url_norm, edit.title.trim(), edit.note, now(), id],
        )?;
        set_host(&tx, id, url)?;
        tx.execute("DELETE FROM bookmark_tag WHERE bookmark_id = ?", [id])?;
        attach_tags(&tx, id, &edit.tags)?;
        prune_tags(&tx)?;
        reindex(&tx, id)?;
        tx.commit()?;
        Ok(changed)
    })()
    .map_err(|e| e.to_string())?;
    if changed == 0 {
        return Err("북마크를 찾을 수 없습니다".into());
    }
    get(conn, id)
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TagCount {
    pub name: String,
    pub count: i64,
}

/// 사이드바에 보여줄 개수들.
#[derive(Debug, Serialize)]
pub struct Sidebar {
    pub total: i64,
    /// 별 4개 이상
    pub starred: i64,
    pub untagged: i64,
    /// `recent_since` 이후에 추가된 것
    pub recent: i64,
    /// 많이 쓴 순서
    pub tags: Vec<TagCount>,
    /// 북마크가 많은 사이트(도메인) 순서
    pub sites: Vec<TagCount>,
}

pub fn sidebar(conn: &Connection, recent_since: i64) -> Result<Sidebar, String> {
    (|| -> rusqlite::Result<Sidebar> {
        let (total, starred, untagged, recent) = conn.query_row(
            "SELECT count(*),
                    count(CASE WHEN b.rating >= 4 THEN 1 END),
                    count(CASE WHEN NOT EXISTS (SELECT 1 FROM bookmark_tag bt WHERE bt.bookmark_id = b.id) THEN 1 END),
                    count(CASE WHEN b.created_at >= ?1 THEN 1 END)
               FROM bookmark b",
            [recent_since],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
        let mut stmt = conn.prepare(
            "SELECT t.name, count(*) FROM tag t JOIN bookmark_tag bt ON bt.tag_id = t.id
              GROUP BY t.id ORDER BY count(*) DESC, t.name COLLATE NOCASE",
        )?;
        let tags = stmt
            .query_map([], |r| Ok(TagCount { name: r.get(0)?, count: r.get(1)? }))?
            .collect::<rusqlite::Result<_>>()?;
        let mut stmt = conn.prepare(
            "SELECT host, count(*) FROM bookmark WHERE host <> ''
              GROUP BY host ORDER BY count(*) DESC, host LIMIT 500",
        )?;
        let sites = stmt
            .query_map([], |r| Ok(TagCount { name: r.get(0)?, count: r.get(1)? }))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(Sidebar { total, starred, untagged, recent, tags, sites })
    })()
    .map_err(|e| e.to_string())
}

/// 가져오기용. 이미 있는 URL 이면 태그만 합치고, 없으면 새로 만든다. (id, 새로 만들었는지) 를 돌려준다.
/// 여러 건을 한 번에 넣으므로 트랜잭션은 호출하는 쪽이 잡는다.
pub(crate) fn import_one(
    conn: &Connection,
    url: &str,
    title: &str,
    note: &str,
    rating: Option<i64>,
    tags: &[String],
    source: &str,
    created_at: Option<i64>,
) -> rusqlite::Result<(i64, bool)> {
    let url_norm = normalize(url);
    let existing: Option<i64> = conn
        .query_row("SELECT id FROM bookmark WHERE url_norm = ?", [&url_norm], |r| r.get(0))
        .optional()?;
    let id = match existing {
        Some(id) => id,
        None => {
            let ts = now();
            conn.execute(
                "INSERT INTO bookmark (url, url_norm, title, note, rating, source, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params![url, url_norm, title.trim(), note, rating, source, created_at.unwrap_or(ts), ts],
            )?;
            conn.last_insert_rowid()
        }
    };
    if existing.is_none() {
        set_host(conn, id, url)?;
    }
    attach_tags(conn, id, tags)?;
    reindex(conn, id)?;
    Ok((id, existing.is_none()))
}

pub fn list(conn: &Connection, filter: &ListFilter) -> Result<Vec<Bookmark>, String> {
    query(conn, filter, filter.limit.unwrap_or(200).clamp(1, 1000))
}

/// 내보내기용. 개수 제한 없이 필터에 맞는 전부.
pub fn list_all(conn: &Connection, filter: &ListFilter) -> Result<Vec<Bookmark>, String> {
    query(conn, filter, -1)
}

/// 필터에 맞는 전체 개수. 화면이 스크롤 길이와 "N개" 표시에 쓴다.
pub fn count(conn: &Connection, filter: &ListFilter) -> Result<i64, String> {
    let (conditions, args) = conditions(filter);
    conn.query_row(
        &format!("SELECT count(*) FROM bookmark b{conditions}"),
        params_from_iter(args),
        |r| r.get(0),
    )
    .map_err(|e| e.to_string())
}

/// 필터를 ` WHERE ...` 절과 그 인자로 바꾼다. 목록과 개수 조회가 같이 쓴다.
fn conditions(filter: &ListFilter) -> (String, Vec<Value>) {
    let mut sql = String::from(" WHERE 1 = 1");
    let mut args: Vec<Value> = Vec::new();

    if let Some(q) = filter.query.as_deref().map(str::trim).filter(|q| !q.is_empty()) {
        if q.chars().count() >= 3 {
            sql.push_str(" AND b.id IN (SELECT rowid FROM bookmark_fts WHERE bookmark_fts MATCH ?)");
            args.push(Value::Text(format!("\"{}\"", q.replace('"', "\"\""))));
        } else {
            // trigram 인덱스는 3글자 미만을 못 찾으므로 LIKE 로 대신한다.
            sql.push_str(
                " AND (b.title LIKE ? OR b.url LIKE ? OR b.note LIKE ?
                       OR EXISTS (SELECT 1 FROM bookmark_tag bt JOIN tag t ON t.id = bt.tag_id
                                   WHERE bt.bookmark_id = b.id AND t.name LIKE ?))",
            );
            let like = format!("%{q}%");
            args.extend(std::iter::repeat_n(Value::Text(like), 4));
        }
    }
    if let Some(tag) = filter.tag.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM bookmark_tag bt JOIN tag t ON t.id = bt.tag_id
                           WHERE bt.bookmark_id = b.id AND t.name = ?)",
        );
        args.push(Value::Text(tag.trim_start_matches('#').to_string()));
    }
    if let Some(min) = filter.min_rating {
        sql.push_str(" AND b.rating >= ?");
        args.push(Value::Integer(min));
    }
    if let Some(host) = filter.host.as_deref().map(str::trim).filter(|h| !h.is_empty()) {
        sql.push_str(" AND b.host = ?");
        args.push(Value::Text(host.to_string()));
    }
    if filter.untagged {
        sql.push_str(" AND NOT EXISTS (SELECT 1 FROM bookmark_tag bt WHERE bt.bookmark_id = b.id)");
    }
    if filter.singleton {
        sql.push_str(" AND b.host <> '' AND (SELECT count(*) FROM bookmark x WHERE x.host = b.host) = 1");
    }
    if filter.unopened {
        sql.push_str(" AND b.open_count = 0");
    }
    if filter.unrated {
        sql.push_str(" AND b.rating IS NULL");
    }
    if filter.no_thumb {
        sql.push_str(" AND NOT EXISTS (SELECT 1 FROM thumbs.thumbnail th WHERE th.bookmark_id = b.id)");
    }
    if let Some(before) = filter.stale_before {
        sql.push_str(" AND coalesce(b.last_opened_at, b.created_at) < ?");
        args.push(Value::Integer(before));
    }
    if let Some(since) = filter.added_since {
        sql.push_str(" AND b.created_at >= ?");
        args.push(Value::Integer(since));
    }
    (sql, args)
}

fn query(conn: &Connection, filter: &ListFilter, limit: i64) -> Result<Vec<Bookmark>, String> {
    let (conditions, mut args) = conditions(filter);
    // 별점 없는 것과 제목 없는 것은 뒤로 보낸다. 마지막 기준은 항상 id 라 이어 읽기 순서가 흔들리지 않는다.
    let order = match filter.sort.as_deref() {
        Some("rating") => "b.rating IS NULL, b.rating DESC, b.created_at DESC, b.id DESC",
        Some("title") => "b.title = '', b.title COLLATE NOCASE, b.id",
        Some("opens") => "b.open_count DESC, b.last_opened_at DESC, b.id DESC",
        Some("oldest") => "b.created_at, b.id",
        _ => "b.created_at DESC, b.id DESC",
    };
    let sql = format!("{SELECT}{conditions} ORDER BY {order} LIMIT ? OFFSET ?");
    args.push(Value::Integer(limit));
    args.push(Value::Integer(filter.offset.unwrap_or(0).max(0)));

    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_from_iter(args), from_row)
        .map_err(|e| e.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())
}

/// 오래 안 연 것으로 보는 기준 (초): 180일
pub const STALE_AFTER: i64 = 180 * 24 * 60 * 60;
const WEEK: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Serialize, PartialEq)]
pub struct MonthCount {
    /// "2026-11" 형식 (이 PC의 현지 시각 기준)
    pub month: String,
    pub count: i64,
}

/// 대시보드에 보여줄 집계.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Dashboard {
    pub total: i64,
    /// 서로 다른 사이트 수와 쓰이고 있는 태그 수
    pub sites: i64,
    pub tags: i64,
    /// 최근 7일에 추가한 수와 그 앞 7일에 추가한 수
    pub recent: i64,
    pub previous: i64,
    pub unopened: i64,
    pub untagged: i64,
    pub unrated: i64,
    /// 썸네일이 없는 것
    pub no_thumb: i64,
    /// 180일 넘게 안 연 것
    pub stale: i64,
    /// 그 사이트의 북마크가 하나뿐인 것
    pub singletons: i64,
    /// 별점별 개수. [0] 은 별점 없음, [1]~[5] 는 그 별점
    pub ratings: [i64; 6],
    /// 가장 자주 연 북마크 (많이 연 순서, 최대 5개)
    pub top: Vec<Bookmark>,
    /// 최근 13개월 안에 추가한 달별 개수. 추가가 없던 달은 빠져 있다.
    pub months: Vec<MonthCount>,
}

pub fn dashboard(conn: &Connection) -> Result<Dashboard, String> {
    dashboard_at(conn, now())
}

fn dashboard_at(conn: &Connection, now: i64) -> Result<Dashboard, String> {
    (|| -> rusqlite::Result<Dashboard> {
        let mut board = conn.query_row(
            "SELECT count(*),
                    count(DISTINCT CASE WHEN b.host <> '' THEN b.host END),
                    count(CASE WHEN b.created_at >= ?1 THEN 1 END),
                    count(CASE WHEN b.created_at >= ?2 AND b.created_at < ?1 THEN 1 END),
                    count(CASE WHEN b.open_count = 0 THEN 1 END),
                    count(CASE WHEN NOT EXISTS (SELECT 1 FROM bookmark_tag bt WHERE bt.bookmark_id = b.id) THEN 1 END),
                    count(CASE WHEN coalesce(b.last_opened_at, b.created_at) < ?3 THEN 1 END),
                    count(CASE WHEN b.rating IS NULL THEN 1 END),
                    count(CASE WHEN b.rating = 1 THEN 1 END), count(CASE WHEN b.rating = 2 THEN 1 END),
                    count(CASE WHEN b.rating = 3 THEN 1 END), count(CASE WHEN b.rating = 4 THEN 1 END),
                    count(CASE WHEN b.rating = 5 THEN 1 END)
               FROM bookmark b",
            params![now - WEEK, now - 2 * WEEK, now - STALE_AFTER],
            |r| {
                let mut ratings = [0i64; 6];
                for (i, slot) in ratings.iter_mut().enumerate() {
                    *slot = r.get(7 + i)?;
                }
                Ok(Dashboard {
                    total: r.get(0)?,
                    sites: r.get(1)?,
                    tags: 0,
                    recent: r.get(2)?,
                    previous: r.get(3)?,
                    unopened: r.get(4)?,
                    untagged: r.get(5)?,
                    stale: r.get(6)?,
                    singletons: 0,
                    no_thumb: 0,
                    unrated: ratings[0],
                    ratings,
                    top: Vec::new(),
                    months: Vec::new(),
                })
            },
        )?;
        board.tags = conn.query_row("SELECT count(DISTINCT tag_id) FROM bookmark_tag", [], |r| r.get(0))?;
        board.singletons = conn.query_row(
            "SELECT count(*) FROM (SELECT 1 FROM bookmark WHERE host <> '' GROUP BY host HAVING count(*) = 1)",
            [],
            |r| r.get(0),
        )?;
        board.no_thumb = conn.query_row(
            "SELECT count(*) FROM bookmark b
              WHERE NOT EXISTS (SELECT 1 FROM thumbs.thumbnail th WHERE th.bookmark_id = b.id)",
            [],
            |r| r.get(0),
        )?;
        board.top = conn
            .prepare(&format!(
                "{SELECT} WHERE b.open_count > 0 ORDER BY b.open_count DESC, b.last_opened_at DESC, b.id DESC LIMIT 5"
            ))?
            .query_map([], from_row)?
            .collect::<rusqlite::Result<_>>()?;
        board.months = conn
            .prepare(
                "SELECT strftime('%Y-%m', created_at, 'unixepoch', 'localtime') AS month, count(*)
                   FROM bookmark WHERE created_at >= ? GROUP BY month ORDER BY month",
            )?
            .query_map([now - 400 * 24 * 60 * 60], |r| Ok(MonthCount { month: r.get(0)?, count: r.get(1)? }))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(board)
    })()
    .map_err(|e| e.to_string())
}

/// 같은 주소로 이미 저장된 북마크. 저장 때와 같은 기준(http/https, 끝 슬래시, 추적 파라미터 무시)으로 찾는다.
pub fn find_by_url(conn: &Connection, url: &str) -> Result<Option<Bookmark>, String> {
    let id: Option<i64> = conn
        .query_row("SELECT id FROM bookmark WHERE url_norm = ?", [normalize(url.trim())], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?;
    id.map(|id| get(conn, id)).transpose()
}

/// 필터에 맞는 북마크의 id 전부. 화면의 "전체 선택"과 조건 일괄 삭제에 쓴다.
pub fn ids(conn: &Connection, filter: &ListFilter) -> Result<Vec<i64>, String> {
    let (conditions, args) = conditions(filter);
    (|| -> rusqlite::Result<Vec<i64>> {
        conn.prepare(&format!("SELECT b.id FROM bookmark b{conditions}"))?
            .query_map(params_from_iter(args), |r| r.get(0))?
            .collect()
    })()
    .map_err(|e| e.to_string())
}

/// 여러 북마크를 한 번에 지운다. 전부 지우거나 하나도 지우지 않는다. 지운 개수를 돌려준다.
pub fn delete_many(conn: &Connection, ids: &[i64]) -> Result<usize, String> {
    (|| -> rusqlite::Result<usize> {
        let tx = conn.unchecked_transaction()?;
        let mut deleted = 0;
        for id in ids {
            tx.execute("DELETE FROM bookmark_fts WHERE rowid = ?", [id])?;
            tx.execute("DELETE FROM thumbs.thumbnail WHERE bookmark_id = ?", [id])?;
            deleted += tx.execute("DELETE FROM bookmark WHERE id = ?", [id])?;
        }
        prune_tags(&tx)?;
        tx.commit()?;
        Ok(deleted)
    })()
    .map_err(|e| e.to_string())
}

/// 북마크를 열었다고 기록한다. 화면이 브라우저로 열기 직전에 호출한다.
pub fn record_open(conn: &Connection, id: i64) -> Result<Bookmark, String> {
    conn.execute(
        "UPDATE bookmark SET open_count = open_count + 1, last_opened_at = ? WHERE id = ?",
        params![now(), id],
    )
    .map_err(|e| e.to_string())?;
    get(conn, id)
}

pub fn set_rating(conn: &Connection, id: i64, rating: Option<i64>) -> Result<Bookmark, String> {
    check_rating(rating)?;
    conn.execute(
        "UPDATE bookmark SET rating = ?, updated_at = ? WHERE id = ?",
        params![rating, now(), id],
    )
    .map_err(|e| e.to_string())?;
    get(conn, id)
}

pub fn delete(conn: &Connection, id: i64) -> Result<(), String> {
    (|| -> rusqlite::Result<()> {
        let tx = conn.unchecked_transaction()?;
        tx.execute("DELETE FROM bookmark_fts WHERE rowid = ?", [id])?;
        tx.execute("DELETE FROM thumbs.thumbnail WHERE bookmark_id = ?", [id])?;
        tx.execute("DELETE FROM bookmark WHERE id = ?", [id])?;
        prune_tags(&tx)?;
        tx.commit()
    })()
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        crate::db::open_in_memory()
    }

    fn new(url: &str, title: &str, tags: &[&str]) -> NewBookmark {
        NewBookmark {
            url: url.into(),
            title: title.into(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn duplicate_url_returns_existing() {
        let conn = db();
        let first = add(&conn, new("https://example.com/a/", "A", &["#dev"])).unwrap();
        assert!(!first.duplicate);
        let second = add(&conn, new("http://example.com/a?utm_source=x", "other", &[])).unwrap();
        assert!(second.duplicate);
        assert_eq!(second.bookmark.id, first.bookmark.id);
        assert_eq!(second.bookmark.title, "A");
        assert_eq!(list(&conn, &ListFilter::default()).unwrap().len(), 1);
    }

    #[test]
    fn search_tag_and_rating_filters() {
        let conn = db();
        let a = add(&conn, new("https://a.example", "러스트 공식 문서", &["dev", "Rust"])).unwrap();
        add(&conn, new("https://b.example", "요리 레시피", &["food"])).unwrap();
        set_rating(&conn, a.bookmark.id, Some(5)).unwrap();

        let by = |f: ListFilter| list(&conn, &f).unwrap();
        // 3글자 이상: FTS, 한글 부분 문자열
        assert_eq!(by(ListFilter { query: Some("공식 문".into()), ..Default::default() }).len(), 1);
        // 3글자 미만: LIKE
        assert_eq!(by(ListFilter { query: Some("요리".into()), ..Default::default() }).len(), 1);
        // 태그는 대소문자 무시, # 허용
        let tagged = by(ListFilter { tag: Some("#rust".into()), ..Default::default() });
        assert_eq!(tagged.len(), 1);
        assert_eq!(tagged[0].tags, vec!["dev", "Rust"]);
        assert_eq!(by(ListFilter { min_rating: Some(4), ..Default::default() }).len(), 1);
    }

    #[test]
    fn delete_removes_search_entry() {
        let conn = db();
        let a = add(&conn, new("https://a.example", "unique-title", &["x"])).unwrap();
        delete(&conn, a.bookmark.id).unwrap();
        let hits = list(&conn, &ListFilter { query: Some("unique".into()), ..Default::default() });
        assert!(hits.unwrap().is_empty());
    }

    #[test]
    fn count_matches_filter_and_paging_is_stable() {
        let conn = db();
        for i in 0..25 {
            let tags: &[&str] = if i % 5 == 0 { &["pick"] } else { &[] };
            add(&conn, new(&format!("https://site{i}.example"), &format!("제목 {i}"), tags)).unwrap();
        }
        assert_eq!(count(&conn, &ListFilter::default()).unwrap(), 25);
        assert_eq!(count(&conn, &ListFilter { tag: Some("pick".into()), ..Default::default() }).unwrap(), 5);
        assert_eq!(count(&conn, &ListFilter { query: Some("site1".into()), ..Default::default() }).unwrap(), 11);

        // 페이지를 이어 읽으면 겹치거나 빠지는 것 없이 전부 나온다.
        let mut seen = Vec::new();
        for offset in (0..30).step_by(10) {
            let page = ListFilter { limit: Some(10), offset: Some(offset), ..Default::default() };
            seen.extend(list(&conn, &page).unwrap().into_iter().map(|b| b.id));
        }
        let mut unique = seen.clone();
        unique.sort();
        unique.dedup();
        assert_eq!((seen.len(), unique.len()), (25, 25));
    }

    /// 대량 데이터 수동 점검. `cargo test large_dataset --release -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn large_dataset() {
        use std::time::Instant;
        const N: usize = 50_000;
        let conn = db();
        let t = Instant::now();
        let tx = conn.unchecked_transaction().unwrap();
        for i in 0..N {
            let tags = vec![format!("tag{}", i % 200), format!("group{}", i % 7)];
            import_one(&tx, &format!("https://host{}.example/page/{i}", i % 900), &format!("문서 제목 {i} 러스트 가이드"),
                       "", None, &tags, "test", Some(1_700_000_000 + i as i64)).unwrap();
        }
        tx.commit().unwrap();
        println!("insert {N}: {:?}", t.elapsed());

        let time = |label: &str, f: ListFilter| {
            let t = Instant::now();
            let total = count(&conn, &f).unwrap();
            let c = t.elapsed();
            let t = Instant::now();
            let rows = list(&conn, &f).unwrap().len();
            println!("{label:<28} count={total:<6} in {c:?}, page of {rows} in {:?}", t.elapsed());
        };
        let page = |offset| ListFilter { limit: Some(200), offset: Some(offset), ..Default::default() };
        time("first page", page(0));
        time("deep page (offset 40000)", page(40_000));
        time("search 3+ chars", ListFilter { query: Some("제목 4999".into()), ..page(0) });
        time("search 2 chars", ListFilter { query: Some("99".into()), ..page(0) });
        time("tag filter", ListFilter { tag: Some("tag17".into()), ..page(0) });
    }

    #[test]
    fn update_replaces_fields_and_tags() {
        let conn = db();
        let a = add(&conn, new("https://a.example", "old", &["one", "two"])).unwrap().bookmark.id;
        add(&conn, new("https://b.example", "other", &["two"])).unwrap();

        let edit = BookmarkEdit {
            url: "https://a.example/new".into(),
            title: " 새 제목 ".into(),
            note: "메모 내용".into(),
            tags: vec!["two".into(), "#three".into()],
        };
        let updated = update(&conn, a, edit).unwrap();
        assert_eq!((updated.title.as_str(), updated.note.as_str()), ("새 제목", "메모 내용"));
        assert_eq!(updated.tags, vec!["two", "three"]);
        // 검색 인덱스도 따라 바뀌고, 아무도 안 쓰는 태그 "one" 은 사라진다.
        assert_eq!(list(&conn, &ListFilter { query: Some("메모 내용".into()), ..Default::default() }).unwrap().len(), 1);
        let names: Vec<String> = sidebar(&conn, 0).unwrap().tags.into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["two", "three"]);

        // 다른 북마크와 같은 주소로는 바꿀 수 없다.
        let clash = BookmarkEdit { url: "http://b.example/".into(), ..Default::default() };
        assert!(update(&conn, a, clash).is_err());
        assert!(update(&conn, 9999, BookmarkEdit { url: "https://x.example".into(), ..Default::default() }).is_err());
    }

    #[test]
    fn sidebar_counts_quick_filters_and_sort() {
        let conn = db();
        let ids: Vec<i64> = [("https://c.example", "다람쥐", &["x"][..]), ("https://a.example", "가방", &[]), ("https://b.example", "", &["x", "y"])]
            .into_iter()
            .map(|(url, title, tags)| add(&conn, new(url, title, tags)).unwrap().bookmark.id)
            .collect();
        set_rating(&conn, ids[0], Some(3)).unwrap();
        set_rating(&conn, ids[1], Some(5)).unwrap();
        conn.execute("UPDATE bookmark SET created_at = 100 WHERE id = ?", [ids[0]]).unwrap();

        let side = sidebar(&conn, 1_000).unwrap();
        assert_eq!((side.total, side.starred, side.untagged, side.recent), (3, 1, 1, 2));
        assert_eq!(side.tags, [TagCount { name: "x".into(), count: 2 }, TagCount { name: "y".into(), count: 1 }]);

        let titles = |f: ListFilter| -> Vec<String> { list(&conn, &f).unwrap().into_iter().map(|b| b.title).collect() };
        assert_eq!(titles(ListFilter { untagged: true, ..Default::default() }), ["가방"]);
        assert_eq!(titles(ListFilter { added_since: Some(1_000), ..Default::default() }).len(), 2);
        assert_eq!(count(&conn, &ListFilter { untagged: true, ..Default::default() }).unwrap(), 1);
        // 별점순: 높은 것부터, 별점 없는 것은 맨 뒤
        assert_eq!(titles(ListFilter { sort: Some("rating".into()), ..Default::default() }), ["가방", "다람쥐", ""]);
        // 제목순: 제목 없는 것은 맨 뒤
        assert_eq!(titles(ListFilter { sort: Some("title".into()), ..Default::default() }), ["가방", "다람쥐", ""]);
        // 오래된 순: 최근 추가순의 반대
        let recent = titles(ListFilter::default());
        let mut oldest = titles(ListFilter { sort: Some("oldest".into()), ..Default::default() });
        assert_eq!(oldest[0], "다람쥐");
        oldest.reverse();
        assert_eq!(oldest, recent);
    }

    #[test]
    fn sites_are_recorded_filtered_and_backfilled() {
        let conn = db();
        let a = add(&conn, new("https://www.github.com/a", "A", &[])).unwrap().bookmark.id;
        add(&conn, new("https://github.com/b", "B", &[])).unwrap();
        add(&conn, new("https://m.youtube.com/watch?v=1", "C", &[])).unwrap();
        add(&conn, new("about:config", "D", &[])).unwrap();
        import_one(&conn, "https://github.com/c", "E", "", None, &[], "chrome", None).unwrap();

        let sites = sidebar(&conn, 0).unwrap().sites;
        assert_eq!(sites, [TagCount { name: "github.com".into(), count: 3 }, TagCount { name: "youtube.com".into(), count: 1 }]);
        let on = |host: &str| ListFilter { host: Some(host.into()), ..Default::default() };
        assert_eq!(count(&conn, &on("github.com")).unwrap(), 3);
        assert_eq!(list(&conn, &on("youtube.com")).unwrap()[0].title, "C");

        // 주소를 바꾸면 사이트도 따라 바뀐다.
        update(&conn, a, BookmarkEdit { url: "https://docs.rs/x".into(), ..Default::default() }).unwrap();
        assert_eq!(count(&conn, &on("github.com")).unwrap(), 2);
        assert_eq!(count(&conn, &on("docs.rs")).unwrap(), 1);

        // host 열이 생기기 전 데이터처럼 비워 놓고 다시 채운다.
        conn.execute("UPDATE bookmark SET host = ''", []).unwrap();
        assert!(sidebar(&conn, 0).unwrap().sites.is_empty());
        backfill_hosts(&conn).unwrap();
        assert_eq!(count(&conn, &on("github.com")).unwrap(), 2);
    }

    #[test]
    fn opens_are_counted_and_sortable() {
        let conn = db();
        let ids: Vec<i64> = ["https://a.example", "https://b.example", "https://c.example"]
            .into_iter()
            .map(|url| add(&conn, new(url, url, &[])).unwrap().bookmark.id)
            .collect();
        assert_eq!(get(&conn, ids[0]).unwrap().open_count, 0);
        assert!(get(&conn, ids[0]).unwrap().last_opened_at.is_none());

        record_open(&conn, ids[1]).unwrap();
        record_open(&conn, ids[1]).unwrap();
        let once = record_open(&conn, ids[0]).unwrap();
        assert_eq!(once.open_count, 1);
        assert!(once.last_opened_at.is_some());
        assert!(record_open(&conn, 9999).is_err());

        let by_opens = list(&conn, &ListFilter { sort: Some("opens".into()), ..Default::default() }).unwrap();
        let counts: Vec<(i64, i64)> = by_opens.iter().map(|b| (b.id, b.open_count)).collect();
        assert_eq!(counts, [(ids[1], 2), (ids[0], 1), (ids[2], 0)]);
        // 제목이나 태그를 고쳐도 횟수는 유지된다.
        let edited = update(&conn, ids[1], BookmarkEdit { url: "https://b.example".into(), ..Default::default() }).unwrap();
        assert_eq!(edited.open_count, 2);
    }

    #[test]
    fn bulk_delete_by_site_leaves_the_rest() {
        let conn = db();
        for i in 0..5 {
            add(&conn, new(&format!("https://spam.example/{i}"), &format!("스팸 {i}"), &["junk"])).unwrap();
        }
        let keep = add(&conn, new("https://keep.example/a", "남길 것", &["good"])).unwrap().bookmark.id;

        let site = ListFilter { host: Some("spam.example".into()), ..Default::default() };
        let targets = ids(&conn, &site).unwrap();
        assert_eq!(targets.len(), 5);
        assert!(!targets.contains(&keep));

        // 이미 없는 id 가 섞여 있어도 있는 것만 지운다.
        let mut with_missing = targets.clone();
        with_missing.push(9999);
        assert_eq!(delete_many(&conn, &with_missing).unwrap(), 5);

        let left = list(&conn, &ListFilter::default()).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].id, keep);
        // 검색 인덱스와 안 쓰는 태그도 함께 정리된다.
        assert!(list(&conn, &ListFilter { query: Some("스팸".into()), ..Default::default() }).unwrap().is_empty());
        let side = sidebar(&conn, 0).unwrap();
        assert_eq!(side.tags, [TagCount { name: "good".into(), count: 1 }]);
        assert_eq!(side.sites, [TagCount { name: "keep.example".into(), count: 1 }]);
        assert_eq!(delete_many(&conn, &[]).unwrap(), 0);
    }

    #[test]
    fn find_by_url_uses_duplicate_rules() {
        let conn = db();
        let saved = add(&conn, new("https://example.com/post/", "글", &["읽을거리"])).unwrap().bookmark;
        let found = find_by_url(&conn, " http://example.com/post?utm_source=mail#top ").unwrap().unwrap();
        assert_eq!(found.id, saved.id);
        assert_eq!(found.tags, vec!["읽을거리"]);
        assert!(find_by_url(&conn, "https://example.com/other").unwrap().is_none());
        assert!(find_by_url(&conn, "").unwrap().is_none());
    }

    #[test]
    fn dashboard_counts_and_matching_filters() {
        let conn = db();
        const DAY: i64 = 24 * 60 * 60;
        let now = 1_800_000_000;
        // (주소, 태그, 별점, 추가한 지 며칠, 연 횟수, 마지막으로 연 지 며칠)
        let rows: [(&str, &[&str], Option<i64>, i64, i64, Option<i64>); 5] = [
            ("https://a.example/1", &["x"], Some(5), 1, 9, Some(0)),
            ("https://a.example/2", &[], None, 3, 0, None),
            ("https://b.example/1", &["x", "y"], Some(3), 10, 2, Some(200)),
            ("https://b.example/2", &[], None, 300, 0, None),
            ("about:config", &["y"], Some(5), 40, 4, Some(5)),
        ];
        for (url, tags, rating, added, opens, last) in rows {
            let id = add(&conn, new(url, url, tags)).unwrap().bookmark.id;
            conn.execute(
                "UPDATE bookmark SET rating = ?, created_at = ?, open_count = ?, last_opened_at = ? WHERE id = ?",
                params![rating, now - added * DAY, opens, last.map(|d| now - d * DAY), id],
            )
            .unwrap();
        }

        let board = dashboard_at(&conn, now).unwrap();
        assert_eq!((board.total, board.sites, board.tags), (5, 2, 2));
        assert_eq!((board.recent, board.previous), (2, 1));
        assert_eq!((board.unopened, board.untagged, board.unrated, board.stale), (2, 2, 2, 2));
        assert_eq!(board.ratings, [2, 0, 0, 1, 0, 2]);
        let top: Vec<i64> = board.top.iter().map(|b| b.open_count).collect();
        assert_eq!(top, [9, 4, 2]);
        assert_eq!(board.months.iter().map(|m| m.count).sum::<i64>(), 5);
        assert!(board.months.iter().all(|m| m.month.len() == 7));

        // 대시보드 숫자를 눌렀을 때 가는 목록의 개수가 그 숫자와 같아야 한다.
        let n = |f: ListFilter| count(&conn, &f).unwrap();
        assert_eq!(n(ListFilter { unopened: true, ..Default::default() }), board.unopened);
        assert_eq!(n(ListFilter { unrated: true, ..Default::default() }), board.unrated);
        assert_eq!(n(ListFilter { stale_before: Some(now - STALE_AFTER), ..Default::default() }), board.stale);
        assert_eq!(n(ListFilter { added_since: Some(now - 7 * DAY), ..Default::default() }), board.recent);

        // 썸네일: 하나에 붙이면 없는 것이 하나 줄고, 목록 개수도 그 숫자와 같다.
        assert_eq!(board.no_thumb, 5);
        let first = list(&conn, &ListFilter::default()).unwrap()[0].id;
        crate::thumbs::set(&conn, first, &[0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        let board = dashboard_at(&conn, now).unwrap();
        assert_eq!(board.no_thumb, 4);
        let without = list(&conn, &ListFilter { no_thumb: true, ..Default::default() }).unwrap();
        assert_eq!(without.len() as i64, board.no_thumb);
        assert!(without.iter().all(|b| b.id != first));

        let empty = dashboard_at(&db(), now).unwrap();
        assert_eq!((empty.total, empty.top.len(), empty.months.len()), (0, 0, 0));
    }

    #[test]
    fn singleton_sites_filter() {
        let conn = db();
        for url in ["https://many.example/1", "https://many.example/2", "https://www.many.example/3"] {
            add(&conn, new(url, url, &[])).unwrap();
        }
        let lone_a = add(&conn, new("https://lone-a.example/x", "혼자 A", &[])).unwrap().bookmark.id;
        let lone_b = add(&conn, new("https://lone-b.example/", "혼자 B", &["keep"])).unwrap().bookmark.id;
        add(&conn, new("about:config", "웹 주소 아님", &[])).unwrap();

        let only = ListFilter { singleton: true, ..Default::default() };
        let mut found = ids(&conn, &only).unwrap();
        found.sort();
        // 웹 주소가 아닌 것은 사이트가 없으므로 대상이 아니다.
        assert_eq!(found, [lone_a, lone_b]);
        assert_eq!(dashboard_at(&conn, 0).unwrap().singletons, 2);

        // 같은 사이트에 하나를 더 저장하면 더 이상 하나뿐이 아니다.
        add(&conn, new("https://lone-a.example/y", "둘째", &[])).unwrap();
        assert_eq!(ids(&conn, &only).unwrap(), [lone_b]);

        // 이 조건으로 지우면 여러 개 있는 사이트는 그대로 남는다.
        delete_many(&conn, &ids(&conn, &only).unwrap()).unwrap();
        assert_eq!(count(&conn, &ListFilter { host: Some("many.example".into()), ..Default::default() }).unwrap(), 3);
        assert_eq!(count(&conn, &ListFilter::default()).unwrap(), 6);
    }

    #[test]
    fn rating_out_of_range_rejected() {
        let conn = db();
        let a = add(&conn, new("https://a.example", "", &[])).unwrap();
        assert!(set_rating(&conn, a.bookmark.id, Some(6)).is_err());
    }
}
