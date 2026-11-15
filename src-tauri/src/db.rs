use std::path::Path;

use rusqlite::Connection;

const MIGRATIONS: &[&str] =
    &[include_str!("../migrations/001_init.sql"), include_str!("../migrations/002_host.sql"),
        include_str!("../migrations/003_open_count.sql"),
    ];

const KEYRING_SERVICE: &str = "bookmark-manage";
/// 키체인 항목 이름: DB 암호화 키
pub const DB_KEY: &str = "db-key";
/// 키체인 항목 이름: 브라우저 확장 연결 토큰
pub const EXTENSION_TOKEN: &str = "extension-token";

/// OS 키체인(윈도우 자격 증명 관리자 / 맥 키체인)에서 비밀값을 읽고, 없으면 새로 만든다.
/// 반환값은 무작위 32바이트의 16진수 문자열.
pub fn load_or_create_secret(name: &str) -> Result<String, String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, name).map_err(|e| e.to_string())?;
    match entry.get_password() {
        Ok(key) => Ok(key),
        Err(keyring::Error::NoEntry) => {
            let mut bytes = [0u8; 32];
            getrandom::getrandom(&mut bytes).map_err(|e| e.to_string())?;
            let key: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
            entry.set_password(&key).map_err(|e| e.to_string())?;
            Ok(key)
        }
        Err(e) => Err(e.to_string()),
    }
}

/// 키체인의 비밀값을 바꾼다. 다른 PC의 백업을 복구 키로 복원했을 때 쓴다.
pub fn store_secret(name: &str, value: &str) -> Result<(), String> {
    keyring::Entry::new(KEYRING_SERVICE, name)
        .and_then(|entry| entry.set_password(value))
        .map_err(|e| format!("키를 저장할 수 없습니다: {e}"))
}

/// 암호화된 DB를 연다. `hex_key` 는 `load_or_create_secret` 이 만든 16진수 키.
pub fn open(path: &Path, hex_key: &str) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    // 키는 직접 생성한 16진수라 문자열 삽입이 안전하다.
    conn.execute_batch(&format!("PRAGMA key = \"x'{hex_key}'\";"))?;
    // 썸네일은 본 DB 를 작게 유지하려고 같은 키로 암호화한 별도 파일에 둔다.
    let thumbs = path.with_file_name("thumbs.db");
    conn.execute(
        &format!("ATTACH DATABASE ?1 AS thumbs KEY \"x'{hex_key}'\""),
        [thumbs.to_string_lossy()],
    )?;
    init(&conn)?;
    Ok(conn)
}

/// 테스트용: 암호화 없는 메모리 DB.
#[cfg(test)]
pub fn open_in_memory() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("ATTACH DATABASE ':memory:' AS thumbs;").unwrap();
    init(&conn).unwrap();
    conn
}

/// PRAGMA 설정과 마이그레이션. `thumbs` 스키마가 먼저 ATTACH 되어 있어야 한다.
fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS thumbs.thumbnail (
             bookmark_id INTEGER PRIMARY KEY,
             data        BLOB    NOT NULL,
             updated_at  INTEGER NOT NULL
         );",
    )?;
    let version: usize = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {};", i + 1))?;
        tx.commit()?;
    }
    if version < 2 {
        // 002 에서 생긴 host 열을 기존 북마크에 채운다.
        crate::bookmarks::backfill_hosts(conn)?;
    }
    Ok(())
}
