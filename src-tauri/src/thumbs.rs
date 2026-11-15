use std::io::Read;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};

use crate::bookmarks::{self, Bookmark};

/// 화면에서 400px 로 줄인 JPEG 만 받으므로 넉넉한 상한.
const MAX_THUMB_BYTES: usize = 1024 * 1024;
/// URL 에서 내려받는 원본 이미지 상한.
const MAX_DOWNLOAD_BYTES: u64 = 20 * 1024 * 1024;

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 리사이즈된 JPEG 를 저장한다. 이미 있으면 교체한다.
pub fn set(conn: &Connection, id: i64, data: &[u8]) -> Result<Bookmark, String> {
    if !data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Err("JPEG 이미지가 아닙니다".into());
    }
    if data.len() > MAX_THUMB_BYTES {
        return Err("썸네일이 너무 큽니다".into());
    }
    bookmarks::get(conn, id)?;
    put(conn, id, data).map_err(|e| e.to_string())?;
    bookmarks::get(conn, id)
}

/// 검증 없이 저장만 한다. 호출하는 쪽이 `is_jpeg` 로 먼저 확인한다.
pub(crate) fn put(conn: &Connection, id: i64, data: &[u8]) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO thumbs.thumbnail (bookmark_id, data, updated_at) VALUES (?, ?, ?)
         ON CONFLICT (bookmark_id) DO UPDATE
            SET data = excluded.data, updated_at = excluded.updated_at",
        params![id, data, now_millis()],
    )?;
    Ok(())
}

pub(crate) fn is_jpeg(data: &[u8]) -> bool {
    data.starts_with(&[0xFF, 0xD8, 0xFF]) && data.len() <= MAX_THUMB_BYTES
}

pub fn get(conn: &Connection, id: i64) -> Result<Vec<u8>, String> {
    conn.query_row("SELECT data FROM thumbs.thumbnail WHERE bookmark_id = ?", [id], |r| r.get(0))
        .optional()
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "썸네일이 없습니다".to_string())
}

pub fn remove(conn: &Connection, id: i64) -> Result<Bookmark, String> {
    conn.execute("DELETE FROM thumbs.thumbnail WHERE bookmark_id = ?", [id])
        .map_err(|e| e.to_string())?;
    bookmarks::get(conn, id)
}

/// 사용자가 입력한 이미지 URL 을 내려받아 원본 바이트를 돌려준다. 리사이즈는 화면에서 한다.
pub fn fetch(url: &str) -> Result<Vec<u8>, String> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err("http 또는 https 주소만 가져올 수 있습니다".into());
    }
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0")
        .build();
    let response = agent.get(url).call().map_err(|e| format!("이미지를 가져오지 못했습니다: {e}"))?;
    let mut data = Vec::new();
    response
        .into_reader()
        .take(MAX_DOWNLOAD_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_DOWNLOAD_BYTES {
        return Err("이미지가 너무 큽니다 (20MB 초과)".into());
    }
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bookmarks::{add, delete, NewBookmark};

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];

    fn bookmark(conn: &Connection) -> i64 {
        let new = NewBookmark { url: "https://a.example".into(), ..Default::default() };
        add(conn, new).unwrap().bookmark.id
    }

    #[test]
    fn set_replace_remove() {
        let conn = crate::db::open_in_memory();
        let id = bookmark(&conn);
        assert!(bookmarks::get(&conn, id).unwrap().thumb_version.is_none());

        assert!(set(&conn, id, JPEG).unwrap().thumb_version.is_some());
        assert_eq!(get(&conn, id).unwrap(), JPEG);

        let other = [JPEG, &[9]].concat();
        set(&conn, id, &other).unwrap();
        assert_eq!(get(&conn, id).unwrap(), other);

        assert!(remove(&conn, id).unwrap().thumb_version.is_none());
        assert!(get(&conn, id).is_err());
    }

    #[test]
    fn rejects_non_jpeg_and_unknown_bookmark() {
        let conn = crate::db::open_in_memory();
        let id = bookmark(&conn);
        assert!(set(&conn, id, b"<svg/>").is_err());
        assert!(set(&conn, id + 1, JPEG).is_err());
    }

    #[test]
    fn deleting_bookmark_drops_thumbnail() {
        let conn = crate::db::open_in_memory();
        let id = bookmark(&conn);
        set(&conn, id, JPEG).unwrap();
        delete(&conn, id).unwrap();
        assert!(get(&conn, id).is_err());
    }

    #[test]
    fn fetch_rejects_non_http() {
        assert!(fetch("file:///c:/windows/win.ini").is_err());
    }
}
