//! 백업과 복원.
//! 스냅샷은 암호화된 DB 파일 두 개(북마크, 썸네일)를 그대로 묶은 파일 하나다.
//! 내용이 이미 암호화되어 있으므로 구글 드라이브 같은 동기화 폴더에 두어도 된다.
//! 사용 중인 DB 를 동기화 폴더에 직접 두지 않고, 완성된 스냅샷만 그 폴더로 옮긴다.

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::db;
use crate::workspaces::{self, safe_id, FOLDER, REGISTRY_FILE};

const PREFIX: &str = "bookmark-manage-";
const EXTENSION: &str = "bmbackup";
const FILES: [&str; 2] = ["bookmarks.db", "thumbs.db"];
/// 자동 백업 간격 (초)
pub const AUTO_INTERVAL: i64 = 24 * 60 * 60;
/// 복원 직전에 현재 데이터를 따로 남겨 두는 개수
const SAFETY_KEEP: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub backup_dir: Option<String>,
    /// 마지막으로 백업에 성공한 시각 (unix 초)
    pub last_backup: Option<i64>,
    /// 백업 폴더에 남겨 둘 스냅샷 개수
    pub keep: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { backup_dir: None, last_backup: None, keep: 10 }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    /// 파일 수정 시각 (unix 초)
    pub modified: i64,
}

#[derive(Debug)]
pub enum RestoreError {
    /// 이 키로는 백업을 열 수 없다. 다른 PC에서 만든 백업이면 그쪽 복구 키가 필요하다.
    WrongKey,
    Other(String),
}

impl<E: Error> From<E> for RestoreError {
    fn from(e: E) -> Self {
        RestoreError::Other(e.to_string())
    }
}

/// 백업에 담는 파일들의 상대 경로 (`/` 구분). 기본 워크스페이스의 DB, 워크스페이스 목록, 나머지 워크스페이스의 DB.
fn data_files(app_dir: &Path) -> Vec<String> {
    let mut files: Vec<String> = FILES.iter().map(|f| f.to_string()).collect();
    if app_dir.join(REGISTRY_FILE).is_file() {
        files.push(REGISTRY_FILE.to_string());
    }
    let mut ids: Vec<String> = fs::read_dir(app_dir.join(FOLDER))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|id| safe_id(id))
        .collect();
    ids.sort();
    for id in ids {
        for file in FILES {
            if app_dir.join(FOLDER).join(&id).join(file).is_file() {
                files.push(format!("{FOLDER}/{id}/{file}"));
            }
        }
    }
    files
}

/// 백업 파일 안의 이름이 우리가 담는 파일인지. 다른 경로로 풀리게 만든 파일은 무시한다.
fn allowed(name: &str) -> bool {
    let parts: Vec<&str> = name.split('/').collect();
    match parts.as_slice() {
        [file] => FILES.contains(file) || *file == REGISTRY_FILE,
        [folder, id, file] => *folder == FOLDER && safe_id(id) && FILES.contains(file),
        _ => false,
    }
}

pub fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// 파일 이름용 시각 `YYYYMMDD-HHMMSS` (UTC). 이름순 정렬이 곧 시간순이 된다.
fn stamp(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // 일수 → 연월일 (그레고리력)
    let z = days + 719_468;
    let (era, doe) = (z.div_euclid(146_097), z.rem_euclid(146_097));
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}{month:02}{day:02}-{:02}{:02}{:02}", rem / 3_600, rem % 3_600 / 60, rem % 60)
}

pub fn is_hex_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

// ---------------------------------------------------------------- 설정

fn settings_path(app_dir: &Path) -> PathBuf {
    app_dir.join("settings.json")
}

pub fn load_settings(app_dir: &Path) -> Settings {
    fs::read_to_string(settings_path(app_dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save_settings(app_dir: &Path, settings: &Settings) -> Result<(), String> {
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(settings_path(app_dir), text).map_err(|e| format!("설정을 저장할 수 없습니다: {e}"))
}

/// 이 PC에서 찾은 구글 드라이브 위치 아래의 백업 폴더 후보.
pub fn suggest_dirs(home: Option<&Path>) -> Vec<String> {
    const MY_DRIVE: [&str; 2] = ["내 드라이브", "My Drive"];
    let mut roots: Vec<PathBuf> = Vec::new();
    if cfg!(windows) {
        // PC용 구글 드라이브는 가상 드라이브(보통 G:)로 붙는다.
        roots.extend(('D'..='Z').map(|letter| PathBuf::from(format!("{letter}:\\"))));
    }
    if let Some(home) = home {
        roots.push(home.join("Google Drive"));
        let cloud = home.join("Library/CloudStorage");
        roots.extend(
            fs::read_dir(cloud)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().starts_with("GoogleDrive"))),
        );
    }
    roots
        .iter()
        .flat_map(|root| MY_DRIVE.iter().map(move |name| root.join(name)))
        .filter(|path| path.is_dir())
        .map(|path| path.join("BookmarkManageBackup").to_string_lossy().into_owned())
        .collect()
}

// ---------------------------------------------------------------- 스냅샷

/// 현재 DB 를 `dest` 폴더에 스냅샷 파일로 남긴다. `conn` 을 잡고 있는 동안 다른 쓰기는 없다.
pub fn snapshot(conn: &Connection, app_dir: &Path, dest: &Path) -> Result<PathBuf, String> {
    // 아직 WAL 에만 있는 변경분을 본 파일로 모두 옮겨야 파일 복사만으로 완전한 사본이 된다.
    for schema in ["main", "thumbs"] {
        conn.query_row(&format!("PRAGMA {schema}.wal_checkpoint(TRUNCATE)"), [], |_| Ok(()))
            .map_err(|e| format!("백업 준비에 실패했습니다: {e}"))?;
    }
    fs::create_dir_all(dest).map_err(|e| format!("백업 폴더를 만들 수 없습니다: {e}"))?;

    let name = format!("{PREFIX}{}.{EXTENSION}", stamp(now()));
    let target = dest.join(&name);
    // 동기화 프로그램이 쓰다 만 파일을 올리지 않도록 임시 이름으로 다 쓴 뒤 바꾼다.
    let partial = dest.join(format!("{name}.tmp"));
    let written = (|| -> Result<(), Box<dyn Error>> {
        let mut zip = ZipWriter::new(fs::File::create(&partial)?);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        for file in data_files(app_dir) {
            zip.start_file(file.as_str(), stored)?;
            io::copy(&mut fs::File::open(app_dir.join(&file))?, &mut zip)?;
        }
        zip.finish()?;
        fs::rename(&partial, &target)?;
        Ok(())
    })();
    if let Err(e) = written {
        let _ = fs::remove_file(&partial);
        return Err(format!("백업 파일을 쓸 수 없습니다: {e}"));
    }
    Ok(target)
}

/// 최신순 스냅샷 목록.
pub fn list(dir: &Path) -> Vec<BackupFile> {
    let mut files: Vec<BackupFile> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(PREFIX) || !name.ends_with(&format!(".{EXTENSION}")) {
                return None;
            }
            let meta = entry.metadata().ok()?;
            let modified = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
            Some(BackupFile { name, path: entry.path().to_string_lossy().into_owned(), size: meta.len(), modified })
        })
        .collect();
    files.sort_by(|a, b| b.name.cmp(&a.name));
    files
}

/// 최신 `keep` 개만 남기고 오래된 스냅샷을 지운다.
pub fn prune(dir: &Path, keep: usize) {
    for old in list(dir).into_iter().skip(keep.max(1)) {
        let _ = fs::remove_file(old.path);
    }
}

// ---------------------------------------------------------------- 복원

fn extract(file: &Path, into: &Path) -> Result<(), Box<dyn Error>> {
    let _ = fs::remove_dir_all(into);
    fs::create_dir_all(into)?;
    let mut archive = ZipArchive::new(fs::File::open(file)?)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().replace('\\', "/");
        if !allowed(&name) {
            continue;
        }
        let target = into.join(&name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        io::copy(&mut entry, &mut fs::File::create(target)?)?;
    }
    if !into.join(FILES[0]).is_file() {
        return Err("북마크 DB 가 들어 있지 않은 파일입니다".into());
    }
    Ok(())
}

/// 스냅샷으로 현재 데이터를 통째로 바꾼다 (병합이 아니다). 복원된 북마크 수를 돌려준다.
/// 바꾸기 직전의 데이터는 `<app_dir>/backups` 에 따로 남긴다.
pub fn restore(slot: &mut Connection, app_dir: &Path, file: &Path, key: &str) -> Result<i64, RestoreError> {
    if !is_hex_key(key) {
        return Err(RestoreError::WrongKey);
    }
    let staging = app_dir.join("restore-tmp");
    extract(file, &staging).map_err(|e| RestoreError::Other(format!("백업 파일을 읽을 수 없습니다: {e}")))?;

    // 실제로 바꾸기 전에 이 키로 열리는지 먼저 확인한다.
    let count = db::open(&staging.join("bookmarks.db"), key)
        .and_then(|conn| conn.query_row("SELECT count(*) FROM bookmark", [], |r| r.get::<_, i64>(0)))
        .map_err(|_| RestoreError::WrongKey)?;

    let safety = app_dir.join("backups");
    snapshot(slot, app_dir, &safety).map_err(RestoreError::Other)?;
    prune(&safety, SAFETY_KEEP);

    // 파일을 바꾸려면 열려 있는 연결을 먼저 닫아야 한다.
    drop(std::mem::replace(slot, Connection::open_in_memory()?));
    // 지금 있는 워크스페이스를 모두 치우고 백업의 것으로 바꾼다.
    for name in FILES {
        for suffix in ["", "-wal", "-shm"] {
            let _ = fs::remove_file(app_dir.join(format!("{name}{suffix}")));
        }
    }
    let _ = fs::remove_dir_all(app_dir.join(FOLDER));
    let _ = fs::remove_file(app_dir.join(REGISTRY_FILE));
    for file in data_files(&staging) {
        let target = app_dir.join(&file);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(staging.join(&file), target)?;
    }
    // 복원한 뒤에는 기본 워크스페이스를 연다.
    let mut registry = workspaces::load(app_dir);
    registry.active = workspaces::DEFAULT_ID.to_string();
    workspaces::save(app_dir, &registry).map_err(RestoreError::Other)?;
    *slot = db::open(&app_dir.join("bookmarks.db"), key)?;
    let _ = fs::remove_dir_all(&staging);
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bookmarks::{add, count, list as list_bookmarks, ListFilter, NewBookmark};
    use crate::thumbs;

    const KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
    const OTHER_KEY: &str = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    const JPEG: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 4, 2];

    /// 테스트마다 따로 쓰는 임시 앱 폴더와 그 안의 암호화 DB.
    fn app(name: &str) -> (PathBuf, Connection) {
        let dir = std::env::temp_dir().join(format!("bm-backup-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let conn = db::open(&dir.join("bookmarks.db"), KEY).unwrap();
        (dir, conn)
    }

    fn save(conn: &Connection, url: &str) -> i64 {
        add(conn, NewBookmark { url: url.into(), ..Default::default() }).unwrap().bookmark.id
    }

    #[test]
    fn stamp_is_utc_calendar_time() {
        assert_eq!(stamp(0), "19700101-000000");
        assert_eq!(stamp(1_700_000_000), "20231114-221320");
        assert_eq!(stamp(1_709_251_199), "20240229-235959");
    }

    #[test]
    fn snapshot_then_restore_brings_back_old_state() {
        let (dir, mut conn) = app("roundtrip");
        let first = save(&conn, "https://first.example");
        thumbs::set(&conn, first, JPEG).unwrap();
        let dest = dir.join("drive");
        let file = snapshot(&conn, &dir, &dest).unwrap();
        assert!(file.is_file() && list(&dest).len() == 1);
        // 스냅샷 파일 자체가 암호화된 내용이어야 한다.
        let raw = fs::read(&file).unwrap();
        assert!(!raw.windows(15).any(|w| w == b"SQLite format 3"));
        assert!(!raw.windows(13).any(|w| w == b"first.example"));

        save(&conn, "https://second.example");
        thumbs::remove(&conn, first).unwrap();
        assert_eq!(count(&conn, &ListFilter::default()).unwrap(), 2);

        assert_eq!(restore(&mut conn, &dir, &file, KEY).unwrap(), 1);
        let all = list_bookmarks(&conn, &ListFilter::default()).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].url, "https://first.example");
        assert_eq!(thumbs::get(&conn, all[0].id).unwrap(), JPEG);
        // 복원 직전 상태(2개)가 안전 사본으로 남아 있고, 복원 후에도 계속 쓸 수 있다.
        assert_eq!(list(&dir.join("backups")).len(), 1);
        save(&conn, "https://third.example");
        assert_eq!(count(&conn, &ListFilter::default()).unwrap(), 2);

        drop(conn);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn backup_carries_every_workspace() {
        let (dir, mut conn) = app("workspaces");
        save(&conn, "https://personal.example");

        // 워크스페이스를 하나 더 만들어 북마크를 넣는다.
        let mut registry = workspaces::load(&dir);
        let work = registry.add("업무").unwrap();
        workspaces::save(&dir, &registry).unwrap();
        let work_dir = workspaces::dir(&dir, &work.id);
        fs::create_dir_all(&work_dir).unwrap();
        let work_conn = db::open(&work_dir.join("bookmarks.db"), KEY).unwrap();
        save(&work_conn, "https://work.example/a");
        save(&work_conn, "https://work.example/b");
        drop(work_conn);

        let file = snapshot(&conn, &dir, &dir.join("drive")).unwrap();

        // 백업 뒤에 업무 워크스페이스를 지우고, 다른 것을 새로 만든다.
        registry.remove(&work.id).unwrap();
        let later = registry.add("나중에 만든 것").unwrap();
        workspaces::save(&dir, &registry).unwrap();
        workspaces::delete_files(&dir, &work.id).unwrap();
        fs::create_dir_all(workspaces::dir(&dir, &later.id)).unwrap();

        assert_eq!(restore(&mut conn, &dir, &file, KEY).unwrap(), 1);
        let restored = workspaces::load(&dir);
        assert_eq!(restored.active, workspaces::DEFAULT_ID);
        assert!(restored.contains(&work.id) && !restored.contains(&later.id));
        assert!(!workspaces::dir(&dir, &later.id).exists());
        let work_conn = db::open(&work_dir.join("bookmarks.db"), KEY).unwrap();
        assert_eq!(count(&work_conn, &ListFilter::default()).unwrap(), 2);
        assert_eq!(count(&conn, &ListFilter::default()).unwrap(), 1);

        drop((conn, work_conn));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_names_outside_our_layout_are_ignored() {
        assert!(allowed("bookmarks.db") && allowed("thumbs.db") && allowed("workspaces.json"));
        assert!(allowed("workspaces/ws-1a/bookmarks.db"));
        for bad in ["../bookmarks.db", "workspaces/../bookmarks.db", "workspaces/ws-1/evil.exe", "settings.json", "a/b/c/d"] {
            assert!(!allowed(bad), "{bad}");
        }
    }

    #[test]
    fn wrong_key_changes_nothing() {
        let (dir, mut conn) = app("wrongkey");
        save(&conn, "https://keep.example");
        let file = snapshot(&conn, &dir, &dir.join("drive")).unwrap();
        save(&conn, "https://newer.example");

        assert!(matches!(restore(&mut conn, &dir, &file, OTHER_KEY), Err(RestoreError::WrongKey)));
        assert!(matches!(restore(&mut conn, &dir, &file, "not-a-key"), Err(RestoreError::WrongKey)));
        assert_eq!(count(&conn, &ListFilter::default()).unwrap(), 2);
        // 잘못된 파일도 현재 데이터를 건드리지 않는다.
        let junk = dir.join("junk.bmbackup");
        fs::write(&junk, b"not a zip").unwrap();
        assert!(matches!(restore(&mut conn, &dir, &junk, KEY), Err(RestoreError::Other(_))));
        assert_eq!(count(&conn, &ListFilter::default()).unwrap(), 2);

        drop(conn);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prune_keeps_newest() {
        let dir = std::env::temp_dir().join(format!("bm-backup-test-{}-prune", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        for day in 1..=5 {
            fs::write(dir.join(format!("{PREFIX}2026010{day}-000000.{EXTENSION}")), b"x").unwrap();
        }
        fs::write(dir.join("unrelated.txt"), b"x").unwrap();
        prune(&dir, 2);
        let left: Vec<String> = list(&dir).into_iter().map(|f| f.name).collect();
        assert_eq!(left, ["bookmark-manage-20260105-000000.bmbackup", "bookmark-manage-20260104-000000.bmbackup"]);
        assert!(dir.join("unrelated.txt").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_round_trip_with_defaults() {
        let dir = std::env::temp_dir().join(format!("bm-backup-test-{}-settings", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let _ = fs::remove_file(settings_path(&dir));
        assert_eq!(load_settings(&dir).keep, 10);
        let mut settings = load_settings(&dir);
        settings.backup_dir = Some("G:\\내 드라이브\\Backup".into());
        save_settings(&dir, &settings).unwrap();
        assert_eq!(load_settings(&dir).backup_dir.as_deref(), Some("G:\\내 드라이브\\Backup"));
        let _ = fs::remove_dir_all(&dir);
    }
}
