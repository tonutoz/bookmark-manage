//! 브라우저 북마크 가져오기 (단방향).
//! - Netscape Bookmark HTML: 모든 브라우저의 "북마크 내보내기" 파일
//! - Chromium `Bookmarks` JSON: 크롬/엣지/브레이브/웨일 프로필 파일
//! - Firefox `places.sqlite`: 파이어폭스/토르 프로필 파일

use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::Serialize;
use serde_json::Value;

use crate::{bookmarks, exporter, thumbs};

#[derive(Debug, Default, PartialEq)]
pub struct Item {
    pub url: String,
    pub title: String,
    /// 바깥쪽부터 안쪽 순서의 폴더 경로. 브라우저 기본 루트(북마크바 등)는 뺀다.
    pub folders: Vec<String>,
    pub tags: Vec<String>,
    /// 브라우저에 추가된 시각 (unix 초).
    pub added: Option<i64>,
    // 아래 셋은 이 앱의 JSON 백업에만 있다.
    pub note: String,
    pub rating: Option<i64>,
    pub thumbnail: Option<Vec<u8>>,
}

#[derive(Debug, Default, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub total: usize,
    /// 새로 추가된 수
    pub added: usize,
    /// 이미 있어서 태그만 합친 수
    pub merged: usize,
    /// javascript: 같은 저장할 수 없는 주소라 건너뛴 수
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
pub struct BrowserSource {
    pub label: String,
    pub path: String,
    pub source: String,
}

// ---------------------------------------------------------------- 저장

fn importable(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    !lower.is_empty() && !["javascript:", "place:", "data:"].iter().any(|p| lower.starts_with(p))
}

/// 폴더 이름을 해시태그처럼 쓸 수 있게 공백을 `_` 로 바꾼다.
fn folder_tag(folder: &str) -> String {
    folder.split_whitespace().collect::<Vec<_>>().join("_")
}

/// 한 트랜잭션으로 전부 넣는다. 중복 URL 은 태그만 합치고 제목·별점·썸네일은 건드리지 않는다.
pub fn import_items(conn: &Connection, items: &[Item], source: &str) -> Result<ImportSummary, String> {
    let mut summary = ImportSummary { total: items.len(), ..Default::default() };
    (|| -> rusqlite::Result<()> {
        let tx = conn.unchecked_transaction()?;
        for item in items {
            if !importable(&item.url) {
                summary.skipped += 1;
                continue;
            }
            let tags: Vec<String> =
                item.folders.iter().map(|f| folder_tag(f)).chain(item.tags.iter().cloned()).collect();
            let rating = item.rating.filter(|r| (1..=5).contains(r));
            let (id, created) = bookmarks::import_one(
                &tx,
                item.url.trim(),
                &item.title,
                &item.note,
                rating,
                &tags,
                source,
                item.added,
            )?;
            if !created {
                summary.merged += 1;
                continue;
            }
            summary.added += 1;
            if let Some(data) = item.thumbnail.as_deref().filter(|d| thumbs::is_jpeg(d)) {
                thumbs::put(&tx, id, data)?;
            }
        }
        tx.commit()
    })()
    .map_err(|e| e.to_string())?;
    Ok(summary)
}

/// 북마크를 다른 워크스페이스로 보낼 때 쓰는 내용. 별점·메모·태그·썸네일·추가 날짜를 그대로 담는다.
pub fn items_from(conn: &Connection, ids: &[i64]) -> Result<Vec<Item>, String> {
    ids.iter()
        .map(|&id| {
            let b = bookmarks::get(conn, id)?;
            let thumbnail = match b.thumb_version {
                Some(_) => Some(thumbs::get(conn, id)?),
                None => None,
            };
            Ok(Item {
                url: b.url,
                title: b.title,
                note: b.note,
                rating: b.rating,
                tags: b.tags,
                added: Some(b.created_at),
                thumbnail,
                folders: Vec::new(),
            })
        })
        .collect()
}

/// 파일 내용을 보고 형식을 판별해 읽는다.
pub fn load(path: &Path) -> Result<Vec<Item>, String> {
    let mut head = [0u8; 16];
    let n = fs::File::open(path)
        .and_then(|mut f| f.read(&mut head))
        .map_err(|e| format!("파일을 열 수 없습니다: {e}"))?;
    if head[..n].starts_with(b"SQLite format 3") {
        return read_places(path);
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&bytes);
    let text = text.trim_start_matches('\u{feff}');
    if text.trim_start().starts_with('{') {
        let doc: Value =
            serde_json::from_str(text).map_err(|e| format!("JSON 파일을 해석할 수 없습니다: {e}"))?;
        if doc["format"] == exporter::JSON_FORMAT {
            parse_backup(&doc)
        } else {
            parse_chromium_doc(&doc)
        }
    } else {
        let items = parse_netscape(&text);
        if items.is_empty() {
            return Err("북마크를 찾지 못했습니다. 북마크 HTML, 크롬 Bookmarks, 파이어폭스 places.sqlite 파일만 지원합니다".into());
        }
        Ok(items)
    }
}

// ---------------------------------------------------------------- Netscape HTML

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
            let name = &rest[1..end];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => {
                    let code = name.strip_prefix('#')?;
                    let n = match code.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => code.parse().ok()?,
                    };
                    char::from_u32(n)
                }
            };
            ch.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// `<` 다음부터 시작하는 문자열에서 태그가 끝나는 `>` 위치. 따옴표 안의 `>` 는 무시한다.
fn tag_end(s: &str) -> Option<usize> {
    let mut quoted = false;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'"' => quoted = !quoted,
            b'>' if !quoted => return Some(i),
            _ => {}
        }
    }
    None
}

/// 태그 안의 속성들. 이름은 소문자로 바꾼다.
fn attributes(tag: &str) -> HashMap<String, String> {
    let mut attrs = HashMap::new();
    let mut rest = tag.trim_start();
    rest = &rest[rest.find(char::is_whitespace).unwrap_or(rest.len())..];
    loop {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        let key_end = rest.find(|c: char| c == '=' || c.is_whitespace()).unwrap_or(rest.len());
        let key = rest[..key_end].to_ascii_lowercase();
        rest = rest[key_end..].trim_start();
        let mut value = String::new();
        if let Some(after) = rest.strip_prefix('=') {
            let after = after.trim_start();
            if let Some(quoted) = after.strip_prefix('"') {
                let end = quoted.find('"').unwrap_or(quoted.len());
                value = quoted[..end].to_string();
                rest = &quoted[(end + 1).min(quoted.len())..];
            } else {
                let end = after.find(char::is_whitespace).unwrap_or(after.len());
                value = after[..end].to_string();
                rest = &after[end..];
            }
        }
        if !key.is_empty() {
            attrs.insert(key, value);
        }
    }
    attrs
}

pub fn parse_netscape(html: &str) -> Vec<Item> {
    let mut items = Vec::new();
    // 열린 <DL> 마다 한 칸. 이름 없는 최상위나 브라우저 기본 루트는 빈 문자열.
    let mut stack: Vec<String> = Vec::new();
    let mut pending: Option<String> = None;
    let mut rest = html;

    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        let Some(gt) = tag_end(rest) else { break };
        let tag = &rest[..gt];
        rest = &rest[gt + 1..];
        let name = tag.split(char::is_whitespace).next().unwrap_or("").to_ascii_lowercase();
        // 제목 안의 `<` 는 내보낼 때 &lt; 로 바뀌므로 다음 `<` 까지가 텍스트다.
        let text = &rest[..rest.find('<').unwrap_or(rest.len())];
        match name.as_str() {
            "h3" => {
                let attrs = attributes(tag);
                let builtin = attrs.contains_key("personal_toolbar_folder")
                    || attrs.contains_key("unfiled_bookmarks_folder");
                pending = Some(if builtin { String::new() } else { decode_entities(text.trim()) });
            }
            "dl" => stack.push(pending.take().unwrap_or_default()),
            "/dl" => {
                stack.pop();
            }
            "a" => {
                let attrs = attributes(tag);
                if let Some(href) = attrs.get("href") {
                    items.push(Item {
                        url: decode_entities(href),
                        title: decode_entities(text.trim()),
                        folders: stack.iter().filter(|s| !s.is_empty()).cloned().collect(),
                        tags: attrs
                            .get("tags")
                            .map(|t| {
                                t.split(',')
                                    .map(|s| decode_entities(s.trim()))
                                    .filter(|s| !s.is_empty())
                                    .collect()
                            })
                            .unwrap_or_default(),
                        added: attrs.get("add_date").and_then(|d| d.parse().ok()),
                        ..Default::default()
                    });
                }
            }
            _ => {}
        }
    }
    items
}

// ---------------------------------------------------------------- Chromium JSON

/// 1601-01-01 기준 마이크로초(WebKit 시각)를 unix 초로.
fn webkit_to_unix(micros: &str) -> Option<i64> {
    let micros: i64 = micros.parse().ok()?;
    (micros > 0).then(|| micros / 1_000_000 - 11_644_473_600)
}

/// 이 앱이 내보낸 JSON 백업.
pub fn parse_backup(doc: &Value) -> Result<Vec<Item>, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let list = doc["bookmarks"].as_array().ok_or("백업 파일에 bookmarks 목록이 없습니다")?;
    Ok(list
        .iter()
        .map(|b| Item {
            url: b["url"].as_str().unwrap_or("").to_string(),
            title: b["title"].as_str().unwrap_or("").to_string(),
            note: b["note"].as_str().unwrap_or("").to_string(),
            rating: b["rating"].as_i64(),
            tags: b["tags"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|t| t.as_str().map(str::to_string))
                .collect(),
            added: b["createdAt"].as_i64(),
            thumbnail: b["thumbnail"].as_str().and_then(|t| STANDARD.decode(t).ok()),
            folders: Vec::new(),
        })
        .collect())
}

#[cfg(test)]
pub fn parse_chromium(json: &str) -> Result<Vec<Item>, String> {
    let doc: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    parse_chromium_doc(&doc)
}

pub fn parse_chromium_doc(doc: &Value) -> Result<Vec<Item>, String> {
    fn walk(node: &Value, path: &mut Vec<String>, out: &mut Vec<Item>) {
        let name = node["name"].as_str().unwrap_or("").to_string();
        match node["type"].as_str() {
            Some("url") => out.push(Item {
                url: node["url"].as_str().unwrap_or("").to_string(),
                title: name,
                folders: path.clone(),
                tags: Vec::new(),
                added: node["date_added"].as_str().and_then(webkit_to_unix),
                ..Default::default()
            }),
            Some("folder") => {
                path.push(name);
                for child in node["children"].as_array().into_iter().flatten() {
                    walk(child, path, out);
                }
                path.pop();
            }
            _ => {}
        }
    }

    let roots = doc["roots"].as_object().ok_or("크롬 Bookmarks 형식이 아닙니다")?;
    let mut items = Vec::new();
    // 루트(북마크바/기타/모바일) 이름은 폴더로 치지 않는다.
    for root in roots.values() {
        for child in root["children"].as_array().into_iter().flatten() {
            walk(child, &mut Vec::new(), &mut items);
        }
    }
    Ok(items)
}

// ---------------------------------------------------------------- Firefox / Tor

const FIREFOX_ROOTS: &[&str] =
    &["root________", "menu________", "toolbar_____", "unfiled_____", "mobile______", "tags________"];

/// 실행 중인 브라우저가 원본을 잠그고 있을 수 있어 임시 폴더에 복사해서 읽는다.
pub fn read_places(path: &Path) -> Result<Vec<Item>, String> {
    let dir = std::env::temp_dir().join(format!("bookmark-manage-import-{}", std::process::id()));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let copy = dir.join("places.sqlite");
    let result = (|| -> Result<Vec<Item>, String> {
        fs::copy(path, &copy).map_err(|e| format!("places.sqlite 를 복사할 수 없습니다: {e}"))?;
        let mut wal = path.as_os_str().to_owned();
        wal.push("-wal");
        if Path::new(&wal).exists() {
            // 아직 본 파일에 반영되지 않은 최근 변경분
            let _ = fs::copy(&wal, dir.join("places.sqlite-wal"));
        }
        let conn = Connection::open(&copy).map_err(|e| e.to_string())?;
        read_places_db(&conn).map_err(|e| format!("파이어폭스 북마크를 읽을 수 없습니다: {e}"))
    })();
    let _ = fs::remove_dir_all(&dir);
    result
}

fn read_places_db(conn: &Connection) -> rusqlite::Result<Vec<Item>> {
    struct Node {
        id: i64,
        kind: i64,
        parent: i64,
        title: String,
        added: Option<i64>,
        guid: String,
        fk: Option<i64>,
        url: Option<String>,
    }

    let mut stmt = conn.prepare(
        "SELECT b.id, b.type, coalesce(b.parent, 0), coalesce(b.title, ''), b.dateAdded,
                coalesce(b.guid, ''), b.fk, p.url
           FROM moz_bookmarks b LEFT JOIN moz_places p ON p.id = b.fk
          ORDER BY b.parent, b.position",
    )?;
    let nodes: Vec<Node> = stmt
        .query_map([], |r| {
            Ok(Node {
                id: r.get(0)?,
                kind: r.get(1)?,
                parent: r.get(2)?,
                title: r.get(3)?,
                added: r.get(4)?,
                guid: r.get(5)?,
                fk: r.get(6)?,
                url: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    let by_id: HashMap<i64, &Node> = nodes.iter().map(|n| (n.id, n)).collect();
    let tags_root = nodes.iter().find(|n| n.guid == "tags________").map(|n| n.id);
    let is_tag_folder = |id: i64| by_id.get(&id).is_some_and(|n| Some(n.parent) == tags_root);

    // 파이어폭스 태그: 태그 루트 아래 폴더가 태그이고, 그 안의 항목이 태그가 붙은 주소다.
    let mut tags_by_place: HashMap<i64, Vec<String>> = HashMap::new();
    for n in &nodes {
        if let (1, Some(fk)) = (n.kind, n.fk) {
            if is_tag_folder(n.parent) {
                tags_by_place.entry(fk).or_default().push(by_id[&n.parent].title.clone());
            }
        }
    }

    let mut items = Vec::new();
    for n in &nodes {
        let (1, Some(url)) = (n.kind, &n.url) else { continue };
        if is_tag_folder(n.parent) {
            continue;
        }
        let mut folders = Vec::new();
        let mut cursor = n.parent;
        for _ in 0..64 {
            let Some(parent) = by_id.get(&cursor) else { break };
            if !FIREFOX_ROOTS.contains(&parent.guid.as_str()) && !parent.title.is_empty() {
                folders.push(parent.title.clone());
            }
            cursor = parent.parent;
        }
        folders.reverse();
        items.push(Item {
            url: url.clone(),
            title: n.title.clone(),
            folders,
            tags: n.fk.and_then(|fk| tags_by_place.get(&fk).cloned()).unwrap_or_default(),
            added: n.added.map(|micros| micros / 1_000_000),
            ..Default::default()
        });
    }
    Ok(items)
}

// ---------------------------------------------------------------- 설치된 브라우저 찾기

fn subdirs_with(base: &Path, file: &str) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| (entry.file_name().to_string_lossy().into_owned(), entry.path().join(file)))
        .filter(|(_, path)| path.is_file())
        .collect();
    found.sort();
    found
}

/// `data_dirs`: 앱 데이터 폴더들 (윈도우의 Local/Roaming, 맥의 Application Support).
/// `desktops`: 토르 브라우저 기본 설치 위치를 찾아볼 바탕화면 폴더들.
pub fn detect(data_dirs: &[PathBuf], desktops: &[PathBuf]) -> Vec<BrowserSource> {
    const CHROMIUM: &[(&str, &str, &[&str])] = &[
        ("Chrome", "chrome", &["Google/Chrome/User Data", "Google/Chrome"]),
        ("Edge", "edge", &["Microsoft/Edge/User Data", "Microsoft Edge"]),
        ("Brave", "brave", &["BraveSoftware/Brave-Browser/User Data", "BraveSoftware/Brave-Browser"]),
        ("Whale", "whale", &["Naver/Naver Whale/User Data", "Naver/Whale"]),
    ];
    const FIREFOX: &[(&str, &str, &[&str])] = &[
        ("Firefox", "firefox", &["Mozilla/Firefox/Profiles", "Firefox/Profiles"]),
        ("Tor Browser", "tor", &["TorBrowser-Data/Browser"]),
    ];

    let mut sources = Vec::new();
    let mut push = |name: &str, source: &str, profile: &str, path: PathBuf| {
        let path = path.to_string_lossy().into_owned();
        if !sources.iter().any(|s: &BrowserSource| s.path == path) {
            sources.push(BrowserSource { label: format!("{name} ({profile})"), path, source: source.into() });
        }
    };

    for (groups, file) in [(CHROMIUM, "Bookmarks"), (FIREFOX, "places.sqlite")] {
        for (name, source, relatives) in groups {
            for base in data_dirs {
                for relative in *relatives {
                    for (profile, path) in subdirs_with(&base.join(relative), file) {
                        push(name, source, &profile, path);
                    }
                }
            }
        }
    }
    for desktop in desktops {
        let profile = desktop.join("Tor Browser/Browser/TorBrowser/Data/Browser/profile.default/places.sqlite");
        if profile.is_file() {
            push("Tor Browser", "tor", "profile.default", profile);
        }
    }
    sources
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bookmarks::{list, ListFilter};

    const HTML: &str = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
<META HTTP-EQUIV="Content-Type" CONTENT="text/html; charset=UTF-8">
<TITLE>Bookmarks</TITLE>
<H1>Bookmarks</H1>
<DL><p>
    <DT><H3 ADD_DATE="1700000000" PERSONAL_TOOLBAR_FOLDER="true">북마크바</H3>
    <DL><p>
        <DT><A HREF="https://top.example/?a=1&amp;b=2" ADD_DATE="1700000001" ICON="data:image/png;base64,AAA>BBB">Top &lt;1&gt;</A>
        <DT><H3 ADD_DATE="1700000002">Web Dev</H3>
        <DL><p>
            <DT><A HREF="https://deep.example/" TAGS="rust,한글">Deep</A>
            <DT><A HREF="javascript:void(0)">북마클릿</A>
        </DL><p>
    </DL><p>
    <DT><A HREF="https://root.example/">Root</A>
</DL><p>
"#;

    #[test]
    fn netscape_folders_tags_and_entities() {
        let items = parse_netscape(HTML);
        assert_eq!(items.len(), 4);
        assert_eq!(
            items[0],
            Item {
                url: "https://top.example/?a=1&b=2".into(),
                title: "Top <1>".into(),
                folders: vec![],
                tags: vec![],
                added: Some(1700000001),
                ..Default::default()
            }
        );
        assert_eq!(items[1].folders, vec!["Web Dev"]);
        assert_eq!(items[1].tags, vec!["rust", "한글"]);
        assert_eq!(items[3].title, "Root");
        assert!(items[3].folders.is_empty());
    }

    #[test]
    fn chromium_json() {
        let json = r#"{"roots":{
            "bookmark_bar":{"type":"folder","name":"북마크바","children":[
                {"type":"url","name":"A","url":"https://a.example/","date_added":"13350000000000000"},
                {"type":"folder","name":"개발","children":[
                    {"type":"folder","name":"Rust","children":[
                        {"type":"url","name":"B","url":"https://b.example/"}]}]}]},
            "other":{"type":"folder","name":"기타","children":[]},
            "sync_transaction_version":"1"}}"#;
        let items = parse_chromium(json).unwrap();
        assert_eq!(items.len(), 2);
        assert!(items[0].folders.is_empty());
        assert_eq!(items[0].added, Some(13_350_000_000 - 11_644_473_600));
        assert_eq!(items[1].folders, vec!["개발", "Rust"]);
        assert!(parse_chromium("{}").is_err());
    }

    #[test]
    fn firefox_places() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT);
             CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, parent INTEGER,
                                         position INTEGER, title TEXT, dateAdded INTEGER, guid TEXT);
             INSERT INTO moz_places VALUES (1, 'https://a.example/'), (2, 'https://b.example/');
             INSERT INTO moz_bookmarks VALUES
               (1, 2, NULL, 0, 0, '',        0, 'root________'),
               (2, 2, NULL, 1, 0, 'menu',    0, 'menu________'),
               (3, 2, NULL, 1, 1, 'toolbar', 0, 'toolbar_____'),
               (4, 2, NULL, 1, 2, 'tags',    0, 'tags________'),
               (5, 2, NULL, 2, 0, '읽을거리', 0, 'aaaaaaaaaaaa'),
               (6, 1, 1,    5, 0, 'A', 1700000000000000, 'bbbbbbbbbbbb'),
               (7, 1, 2,    3, 0, 'B', NULL, 'cccccccccccc'),
               (8, 2, NULL, 4, 0, 'news',    0, 'dddddddddddd'),
               (9, 1, 1,    8, 0, NULL,      0, 'eeeeeeeeeeee'),
               (10, 3, NULL, 3, 1, NULL,     0, 'ffffffffffff');",
        )
        .unwrap();
        let mut items = read_places_db(&conn).unwrap();
        items.sort_by(|a, b| a.title.cmp(&b.title));
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].folders, vec!["읽을거리"]);
        assert_eq!(items[0].tags, vec!["news"]);
        assert_eq!(items[0].added, Some(1700000000));
        assert!(items[1].folders.is_empty() && items[1].tags.is_empty());
    }

    /// 이 PC의 실제 브라우저 프로필로 돌려보는 수동 점검. 개수만 출력한다.
    /// `cargo test real_profiles -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_profiles() {
        let dirs = |names: &[&str]| -> Vec<PathBuf> {
            names.iter().filter_map(|n| std::env::var_os(n)).map(PathBuf::from).collect()
        };
        let desktops: Vec<PathBuf> = dirs(&["USERPROFILE", "HOME"])
            .into_iter()
            .flat_map(|home| [home.join("Desktop"), home.join("OneDrive").join("Desktop")])
            .collect();
        let sources = detect(&dirs(&["LOCALAPPDATA", "APPDATA"]), &desktops);
        println!("found {} profile(s)", sources.len());
        for s in sources {
            match load(Path::new(&s.path)) {
                Ok(items) => {
                    let conn = crate::db::open_in_memory();
                    let summary = import_items(&conn, &items, &s.source).unwrap();
                    let foldered = items.iter().filter(|i| !i.folders.is_empty()).count();
                    println!("{}: {summary:?}, in folders: {foldered}", s.label);
                }
                Err(e) => println!("{}: ERROR {e}", s.label),
            }
        }
    }

    #[test]
    fn moving_to_another_workspace_keeps_everything() {
        use crate::bookmarks::{add, count, delete_many, set_rating, update, BookmarkEdit, NewBookmark};
        const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 1, 1];
        let personal = crate::db::open_in_memory();
        let work = crate::db::open_in_memory();
        let save = |conn: &Connection, url: &str, tags: &[&str]| {
            let new = NewBookmark { url: url.into(), title: url.into(), tags: tags.iter().map(|t| t.to_string()).collect(), ..Default::default() };
            add(conn, new).unwrap().bookmark.id
        };
        let a = save(&personal, "https://jira.example/1", &["회사"]);
        let b = save(&personal, "https://wiki.example/x", &[]);
        save(&personal, "https://hobby.example", &[]);
        set_rating(&personal, a, Some(4)).unwrap();
        update(&personal, a, BookmarkEdit { url: "https://jira.example/1".into(), title: "이슈".into(), note: "메모".into(), tags: vec!["회사".into()] }).unwrap();
        thumbs::put(&personal, a, JPEG).unwrap();
        // 받는 쪽에 같은 주소가 이미 있는 경우
        save(&work, "http://wiki.example/x/", &["기존"]);

        let ids = [a, b];
        let summary = import_items(&work, &items_from(&personal, &ids).unwrap(), "move").unwrap();
        assert_eq!((summary.added, summary.merged), (1, 1));
        assert_eq!(delete_many(&personal, &ids).unwrap(), 2);

        assert_eq!(count(&personal, &ListFilter::default()).unwrap(), 1);
        let moved = list(&work, &ListFilter { query: Some("이슈".into()), ..Default::default() }).unwrap();
        assert_eq!(moved.len(), 1);
        assert_eq!((moved[0].rating, moved[0].note.as_str()), (Some(4), "메모"));
        assert_eq!(moved[0].tags, vec!["회사"]);
        assert_eq!(thumbs::get(&work, moved[0].id).unwrap(), JPEG);
        assert_eq!(count(&work, &ListFilter::default()).unwrap(), 2);
    }

    #[test]
    fn import_merges_duplicates_and_skips_unsavable() {
        let conn = crate::db::open_in_memory();
        let items = parse_netscape(HTML);
        let first = import_items(&conn, &items, "chrome").unwrap();
        assert_eq!(first, ImportSummary { total: 4, added: 3, merged: 0, skipped: 1 });

        // 다시 가져오면 전부 중복이고, 새 폴더는 태그로만 합쳐진다.
        let again = vec![Item {
            url: "http://deep.example".into(),
            title: "다른 제목".into(),
            folders: vec!["Read Later".into()],
            ..Default::default()
        }];
        let second = import_items(&conn, &again, "firefox").unwrap();
        assert_eq!(second, ImportSummary { total: 1, added: 0, merged: 1, skipped: 0 });

        let all = list(&conn, &ListFilter::default()).unwrap();
        assert_eq!(all.len(), 3);
        let deep = all.iter().find(|b| b.url == "https://deep.example/").unwrap();
        assert_eq!(deep.title, "Deep");
        assert_eq!(deep.tags, vec!["Web_Dev", "rust", "한글", "Read_Later"]);
        // 브라우저에 추가된 시각을 유지한다.
        assert_eq!(all.iter().find(|b| b.title == "Top <1>").unwrap().created_at, 1700000001);
    }
}
