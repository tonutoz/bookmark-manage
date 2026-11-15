//! 내보내기.
//! - Netscape Bookmark HTML: 브라우저로 다시 넣는 용도. 별점·메모·썸네일은 담을 수 없다.
//! - JSON: 전체 데이터(별점·메모·썸네일 포함)를 담는 백업. 암호화되지 않는다.

use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD, Engine};
use rusqlite::Connection;
use serde_json::json;

use crate::bookmarks::Bookmark;
use crate::thumbs;

pub const JSON_FORMAT: &str = "bookmark-manage";

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn link(b: &Bookmark, indent: &str) -> String {
    let tags = if b.tags.is_empty() {
        String::new()
    } else {
        format!(" TAGS=\"{}\"", escape(&b.tags.join(",")))
    };
    format!(
        "{indent}<DT><A HREF=\"{}\" ADD_DATE=\"{}\"{tags}>{}</A>\n",
        escape(&b.url),
        b.created_at,
        escape(if b.title.is_empty() { &b.url } else { &b.title })
    )
}

/// 브라우저는 태그 대신 폴더를 쓰므로 첫 번째 태그를 폴더로 삼는다.
/// 태그 전체는 TAGS 속성에도 넣는다 (파이어폭스·토르가 읽는다).
pub fn to_netscape(items: &[Bookmark]) -> String {
    let mut folders: BTreeMap<&str, Vec<&Bookmark>> = BTreeMap::new();
    let mut loose = Vec::new();
    for b in items {
        match b.tags.first() {
            Some(tag) => folders.entry(tag).or_default().push(b),
            None => loose.push(b),
        }
    }

    let mut out = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n\
         <!-- This is an automatically generated file. -->\n\
         <META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n\
         <TITLE>Bookmarks</TITLE>\n\
         <H1>Bookmarks</H1>\n\
         <DL><p>\n",
    );
    for (folder, list) in folders {
        out.push_str(&format!("    <DT><H3>{}</H3>\n    <DL><p>\n", escape(folder)));
        for b in list {
            out.push_str(&link(b, "        "));
        }
        out.push_str("    </DL><p>\n");
    }
    for b in loose {
        out.push_str(&link(b, "    "));
    }
    out.push_str("</DL><p>\n");
    out
}

pub fn to_json(conn: &Connection, items: &[Bookmark], exported_at: i64) -> Result<String, String> {
    let mut bookmarks = Vec::with_capacity(items.len());
    for b in items {
        let thumbnail = match b.thumb_version {
            Some(_) => Some(STANDARD.encode(thumbs::get(conn, b.id)?)),
            None => None,
        };
        bookmarks.push(json!({
            "url": b.url,
            "title": b.title,
            "note": b.note,
            "rating": b.rating,
            "tags": b.tags,
            "createdAt": b.created_at,
            "thumbnail": thumbnail,
        }));
    }
    serde_json::to_string_pretty(&json!({
        "format": JSON_FORMAT,
        "version": 1,
        "exportedAt": exported_at,
        "bookmarks": bookmarks,
    }))
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bookmarks::{add, list_all, set_rating, ListFilter, NewBookmark};
    use crate::importer::{import_items, parse_backup, parse_netscape};

    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 7, 7];

    fn seeded() -> Connection {
        let conn = crate::db::open_in_memory();
        let new = |url: &str, title: &str, note: &str, tags: &[&str]| {
            let b = NewBookmark {
                url: url.into(),
                title: title.into(),
                note: note.into(),
                tags: tags.iter().map(|t| t.to_string()).collect(),
                ..Default::default()
            };
            add(&conn, b).unwrap().bookmark.id
        };
        let a = new("https://a.example/?x=1&y=2", "A <\"quoted\"> & co", "메모", &["dev", "rust"]);
        new("https://b.example/", "", "", &[]);
        set_rating(&conn, a, Some(4)).unwrap();
        thumbs::set(&conn, a, JPEG).unwrap();
        conn
    }

    #[test]
    fn netscape_round_trip() {
        let conn = seeded();
        let html = to_netscape(&list_all(&conn, &ListFilter::default()).unwrap());
        let mut items = parse_netscape(&html);
        items.sort_by(|x, y| x.url.cmp(&y.url));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].url, "https://a.example/?x=1&y=2");
        assert_eq!(items[0].title, "A <\"quoted\"> & co");
        assert_eq!(items[0].folders, vec!["dev"]);
        assert_eq!(items[0].tags, vec!["dev", "rust"]);
        assert!(items[0].added.is_some());
        // 제목이 없으면 주소를 제목으로 쓴다.
        assert_eq!(items[1].title, "https://b.example/");
        assert!(items[1].folders.is_empty());
    }

    #[test]
    fn json_backup_restores_everything() {
        let conn = seeded();
        let json = to_json(&conn, &list_all(&conn, &ListFilter::default()).unwrap(), 123).unwrap();

        let restored = crate::db::open_in_memory();
        let doc = serde_json::from_str(&json).unwrap();
        let summary = import_items(&restored, &parse_backup(&doc).unwrap(), "backup").unwrap();
        assert_eq!((summary.added, summary.merged, summary.skipped), (2, 0, 0));

        let all = list_all(&restored, &ListFilter::default()).unwrap();
        let a = all.iter().find(|b| b.url.starts_with("https://a.")).unwrap();
        assert_eq!(a.note, "메모");
        assert_eq!(a.rating, Some(4));
        assert_eq!(a.tags, vec!["dev", "rust"]);
        assert_eq!(thumbs::get(&restored, a.id).unwrap(), JPEG);
        // 메모도 검색 인덱스에 들어간다.
        let found = ListFilter { query: Some("메모".into()), ..Default::default() };
        assert_eq!(crate::bookmarks::list(&restored, &found).unwrap().len(), 1);
    }
}
