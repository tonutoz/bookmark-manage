mod backup;
mod bookmarks;
mod db;
mod exporter;
mod extension_server;
mod importer;
mod thumbs;
mod tray;
mod url_norm;
mod workspaces;

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bookmarks::{Bookmark, BookmarkEdit, ListFilter, NewBookmark, SaveResult};
use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager, State};

pub(crate) struct Db(Mutex<Connection>);

/// 백업·복원에 필요한 앱 데이터 폴더와 현재 DB 키.
struct BackupCtx {
    dir: PathBuf,
    key: Mutex<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupStatus {
    dir: Option<String>,
    last_backup: Option<i64>,
    keep: usize,
    /// 이 PC에서 찾은 구글 드라이브 안의 백업 폴더 후보
    suggestions: Vec<String>,
    backups: Vec<backup::BackupFile>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreOutcome {
    restored: Option<i64>,
    /// 이 PC의 키로는 열 수 없는 백업이라 복구 키 입력이 필요하다.
    need_key: bool,
}

/// 설정된 백업 폴더에 스냅샷을 남기고 오래된 것을 정리한다.
fn run_backup(app: &AppHandle) -> Result<String, String> {
    let ctx = app.state::<BackupCtx>();
    let mut settings = backup::load_settings(&ctx.dir);
    let dest = settings.backup_dir.clone().ok_or("백업 폴더가 설정되지 않았습니다")?;
    let dest = Path::new(&dest);
    let file = app.state::<Db>().with(|conn| backup::snapshot(conn, &ctx.dir, dest))?;
    backup::prune(dest, settings.keep);
    settings.last_backup = Some(backup::now());
    backup::save_settings(&ctx.dir, &settings)?;
    Ok(file.to_string_lossy().into_owned())
}

/// 앱이 떠 있는 동안 하루에 한 번 자동으로 백업한다.
fn spawn_auto_backup(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(60));
        loop {
            let settings = backup::load_settings(&app.state::<BackupCtx>().dir);
            let due = settings.last_backup.is_none_or(|last| backup::now() - last >= backup::AUTO_INTERVAL);
            if settings.backup_dir.is_some() && due {
                let _ = run_backup(&app);
            }
            std::thread::sleep(std::time::Duration::from_secs(60 * 60));
        }
    });
}

#[tauri::command]
fn backup_status(app: AppHandle, ctx: State<BackupCtx>) -> BackupStatus {
    let settings = backup::load_settings(&ctx.dir);
    BackupStatus {
        backups: settings.backup_dir.as_deref().map(|d| backup::list(Path::new(d))).unwrap_or_default(),
        suggestions: backup::suggest_dirs(app.path().home_dir().ok().as_deref()),
        dir: settings.backup_dir,
        last_backup: settings.last_backup,
        keep: settings.keep,
    }
}

#[tauri::command]
fn set_backup_dir(ctx: State<BackupCtx>, dir: Option<String>) -> Result<(), String> {
    let mut settings = backup::load_settings(&ctx.dir);
    settings.backup_dir = dir.map(|d| d.trim().to_string()).filter(|d| !d.is_empty());
    backup::save_settings(&ctx.dir, &settings)
}

#[tauri::command]
async fn backup_now(app: AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || run_backup(&app)).await.map_err(|e| e.to_string())?
}

/// 다른 PC나 재설치한 윈도우에서 백업을 열 때 필요한 키. 화면에서 복사해 안전한 곳에 보관한다.
#[tauri::command]
fn recovery_key(ctx: State<BackupCtx>) -> Result<String, String> {
    ctx.key.lock().map(|k| k.clone()).map_err(|e| e.to_string())
}

/// 스냅샷으로 현재 데이터를 통째로 바꾼다. key 를 주지 않으면 이 PC의 키로 시도한다.
#[tauri::command]
async fn restore_backup(app: AppHandle, path: String, key: Option<String>) -> Result<RestoreOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let ctx = app.state::<BackupCtx>();
        let current = ctx.key.lock().map_err(|e| e.to_string())?.clone();
        let key = key.map(|k| k.trim().to_lowercase()).filter(|k| !k.is_empty()).unwrap_or_else(|| current.clone());

        let db = app.state::<Db>();
        let mut conn = db.0.lock().map_err(|e| e.to_string())?;
        let restored = match backup::restore(&mut conn, &ctx.dir, Path::new(&path), &key) {
            Ok(count) => count,
            Err(backup::RestoreError::WrongKey) => return Ok(RestoreOutcome { restored: None, need_key: true }),
            Err(backup::RestoreError::Other(e)) => return Err(e),
        };
        drop(conn);
        // 백업에 들어 있던 워크스페이스 목록으로 바뀌었으므로 다시 읽는다.
        *app.state::<Workspaces>().0.lock().map_err(|e| e.to_string())? = workspaces::load(&ctx.dir);
        if key != current {
            // 이제 DB 가 백업의 키로 암호화되어 있으므로 이 PC의 키도 그것으로 바꾼다.
            db::store_secret(db::DB_KEY, &key)?;
            *ctx.key.lock().map_err(|e| e.to_string())? = key;
        }
        let _ = app.emit(extension_server::CHANGED_EVENT, ());
        Ok(RestoreOutcome { restored: Some(restored), need_key: false })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 워크스페이스 목록과 지금 열려 있는 것. `Db` 는 항상 열려 있는 워크스페이스의 DB 를 가리킨다.
struct Workspaces(Mutex<workspaces::Registry>);

impl Workspaces {
    fn snapshot(&self) -> Result<workspaces::Registry, String> {
        self.0.lock().map(|r| r.clone()).map_err(|e| e.to_string())
    }

    /// 목록을 고치고 파일에 저장한다. 저장에 실패하면 고친 내용도 되돌린다.
    fn change<T>(
        &self,
        app_dir: &Path,
        f: impl FnOnce(&mut workspaces::Registry) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut registry = self.0.lock().map_err(|e| e.to_string())?;
        let before = registry.clone();
        let result = f(&mut registry).and_then(|value| workspaces::save(app_dir, &registry).map(|_| value));
        if result.is_err() {
            *registry = before;
        }
        result
    }
}

fn db_key(app: &AppHandle) -> Result<String, String> {
    app.state::<BackupCtx>().key.lock().map(|k| k.clone()).map_err(|e| e.to_string())
}

/// 워크스페이스의 DB 를 연다. 처음 여는 것이면 폴더와 DB 가 이때 만들어진다.
fn open_workspace_db(app: &AppHandle, id: &str) -> Result<Connection, String> {
    let dir = workspaces::dir(&app.state::<BackupCtx>().dir, id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    db::open(&dir.join("bookmarks.db"), &db_key(app)?).map_err(|e| e.to_string())
}

#[tauri::command]
fn list_workspaces(state: State<Workspaces>) -> Result<workspaces::Registry, String> {
    state.snapshot()
}

#[tauri::command]
fn create_workspace(app: AppHandle, name: String) -> Result<workspaces::Workspace, String> {
    let app_dir = app.state::<BackupCtx>().dir.clone();
    app.state::<Workspaces>().change(&app_dir, |registry| registry.add(&name))
}

#[tauri::command]
fn rename_workspace(app: AppHandle, id: String, name: String) -> Result<(), String> {
    let app_dir = app.state::<BackupCtx>().dir.clone();
    app.state::<Workspaces>().change(&app_dir, |registry| registry.rename(&id, &name))
}

/// 워크스페이스와 그 안의 북마크를 모두 지운다. 열려 있는 것과 기본 워크스페이스는 지울 수 없다.
#[tauri::command]
fn delete_workspace(app: AppHandle, id: String) -> Result<(), String> {
    let app_dir = app.state::<BackupCtx>().dir.clone();
    app.state::<Workspaces>().change(&app_dir, |registry| registry.remove(&id))?;
    workspaces::delete_files(&app_dir, &id)
}

/// 다른 워크스페이스로 전환한다. 열어 둔 DB 를 닫고 그 워크스페이스의 DB 를 연다.
#[tauri::command]
fn switch_workspace(app: AppHandle, id: String) -> Result<(), String> {
    let app_dir = app.state::<BackupCtx>().dir.clone();
    let state = app.state::<Workspaces>();
    let previous = state.snapshot()?;
    if !previous.contains(&id) {
        return Err("없는 워크스페이스입니다".into());
    }
    if previous.active == id {
        return Ok(());
    }
    {
        let db = app.state::<Db>();
        let mut conn = db.0.lock().map_err(|e| e.to_string())?;
        // 파일을 놓아야 하므로 먼저 닫는다. 새 것을 못 열면 원래 것을 다시 연다.
        drop(std::mem::replace(&mut *conn, Connection::open_in_memory().map_err(|e| e.to_string())?));
        match open_workspace_db(&app, &id) {
            Ok(opened) => *conn = opened,
            Err(e) => {
                *conn = open_workspace_db(&app, &previous.active)?;
                return Err(format!("워크스페이스를 열 수 없습니다: {e}"));
            }
        }
    }
    state.change(&app_dir, |registry| {
        registry.active = id.clone();
        Ok(())
    })?;
    let _ = app.emit(extension_server::CHANGED_EVENT, ());
    Ok(())
}

/// 지금 워크스페이스의 북마크들을 다른 워크스페이스로 보낸다. 별점·메모·태그·썸네일이 함께 간다.
/// 받는 쪽에 같은 주소가 이미 있으면 태그만 합친다. 보낸 개수를 돌려준다.
#[tauri::command]
fn move_bookmarks(app: AppHandle, ids: Vec<i64>, target: String) -> Result<usize, String> {
    let registry = app.state::<Workspaces>().snapshot()?;
    if !registry.contains(&target) || registry.active == target {
        return Err("보낼 수 없는 워크스페이스입니다".into());
    }
    app.state::<Db>().with(|source| {
        let items = importer::items_from(source, &ids)?;
        let destination = open_workspace_db(&app, &target)?;
        importer::import_items(&destination, &items, "move")?;
        drop(destination);
        // 받는 쪽에 다 들어간 뒤에만 이쪽에서 지운다.
        bookmarks::delete_many(source, &ids)
    })
}

/// 확장 프로그램의 요청을 처리한다. 요청이 워크스페이스를 지정하면 그곳에, 아니면 열려 있는 곳에 저장한다.
pub(crate) fn serve_extension(app: &AppHandle, req: &extension_server::Req) -> (u16, serde_json::Value) {
    use serde_json::json;
    let registry = match app.state::<Workspaces>().snapshot() {
        Ok(registry) => registry,
        Err(e) => return (500, json!({ "error": e })),
    };
    if (req.method, req.path) == ("GET", "/workspaces") {
        return (200, json!({ "active": registry.active, "items": registry.items }));
    }
    // 모르는 id 가 오면(지워진 워크스페이스 등) 열려 있는 곳에 저장한다.
    let elsewhere = serde_json::from_str::<serde_json::Value>(req.body)
        .ok()
        .and_then(|body| body["workspace"].as_str().map(str::to_string))
        .filter(|id| registry.contains(id) && *id != registry.active);
    let result = match elsewhere {
        None => app.state::<Db>().with(|conn| Ok(extension_server::dispatch(conn, req))),
        // 보고 있지 않은 워크스페이스는 그 DB 를 잠깐 열어 처리하고 닫는다.
        Some(id) => open_workspace_db(app, &id).map(|conn| extension_server::dispatch(&conn, req)),
    };
    result.unwrap_or_else(|e| (500, json!({ "error": e })))
}

/// 브라우저 확장 연결 상태. 화면에 토큰을 보여주는 데 쓴다.
#[derive(Clone, serde::Serialize)]
struct ExtensionInfo {
    port: u16,
    token: String,
    /// 수신 서버를 띄우지 못했을 때의 이유
    error: Option<String>,
}

impl Db {
    pub(crate) fn with<T>(&self, f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
        let conn = self.0.lock().map_err(|e| e.to_string())?;
        f(&conn)
    }
}

#[tauri::command]
fn add_bookmark(db: State<Db>, bookmark: NewBookmark) -> Result<SaveResult, String> {
    db.with(|c| bookmarks::add(c, bookmark))
}

#[tauri::command]
fn list_bookmarks(db: State<Db>, filter: Option<ListFilter>) -> Result<Vec<Bookmark>, String> {
    db.with(|c| bookmarks::list(c, &filter.unwrap_or_default()))
}

#[tauri::command]
fn count_bookmarks(db: State<Db>, filter: Option<ListFilter>) -> Result<i64, String> {
    db.with(|c| bookmarks::count(c, &filter.unwrap_or_default()))
}

#[tauri::command]
fn update_bookmark(db: State<Db>, id: i64, edit: BookmarkEdit) -> Result<Bookmark, String> {
    db.with(|c| bookmarks::update(c, id, edit))
}

#[tauri::command]
fn sidebar_counts(db: State<Db>, recent_since: i64) -> Result<bookmarks::Sidebar, String> {
    db.with(|c| bookmarks::sidebar(c, recent_since))
}

#[tauri::command]
fn dashboard(db: State<Db>) -> Result<bookmarks::Dashboard, String> {
    db.with(bookmarks::dashboard)
}

#[tauri::command]
fn open_bookmark(db: State<Db>, id: i64) -> Result<Bookmark, String> {
    db.with(|c| bookmarks::record_open(c, id))
}

#[tauri::command]
fn set_rating(db: State<Db>, id: i64, rating: Option<i64>) -> Result<Bookmark, String> {
    db.with(|c| bookmarks::set_rating(c, id, rating))
}

#[tauri::command]
fn delete_bookmark(db: State<Db>, id: i64) -> Result<(), String> {
    db.with(|c| bookmarks::delete(c, id))
}

#[tauri::command]
fn delete_bookmarks(db: State<Db>, ids: Vec<i64>) -> Result<usize, String> {
    db.with(|c| bookmarks::delete_many(c, &ids))
}

#[tauri::command]
fn list_bookmark_ids(db: State<Db>, filter: Option<ListFilter>) -> Result<Vec<i64>, String> {
    db.with(|c| bookmarks::ids(c, &filter.unwrap_or_default()))
}

#[tauri::command]
fn find_bookmark(db: State<Db>, url: String) -> Result<Option<Bookmark>, String> {
    db.with(|c| bookmarks::find_by_url(c, &url))
}

#[tauri::command]
fn set_thumbnail(db: State<Db>, id: i64, data: Vec<u8>) -> Result<Bookmark, String> {
    db.with(|c| thumbs::set(c, id, &data))
}

#[tauri::command]
fn get_thumbnail(db: State<Db>, id: i64) -> Result<tauri::ipc::Response, String> {
    db.with(|c| thumbs::get(c, id)).map(tauri::ipc::Response::new)
}

#[tauri::command]
fn remove_thumbnail(db: State<Db>, id: i64) -> Result<Bookmark, String> {
    db.with(|c| thumbs::remove(c, id))
}

/// 이미지 URL 의 원본 바이트를 돌려준다. 네트워크 대기가 있어 별도 스레드에서 돈다.
#[tauri::command]
async fn fetch_image(url: String) -> Result<tauri::ipc::Response, String> {
    tauri::async_runtime::spawn_blocking(move || thumbs::fetch(&url))
        .await
        .map_err(|e| e.to_string())?
        .map(tauri::ipc::Response::new)
}

/// 이 PC에서 찾은 브라우저 프로필 목록.
#[tauri::command]
fn detect_browsers(app: AppHandle) -> Vec<importer::BrowserSource> {
    let paths = app.path();
    let data_dirs: Vec<_> = [paths.local_data_dir(), paths.data_dir()].into_iter().flatten().collect();
    let mut desktops: Vec<_> = paths.desktop_dir().into_iter().collect();
    if let Ok(home) = paths.home_dir() {
        desktops.push(home.join("Desktop"));
        desktops.push(home.join("OneDrive").join("Desktop"));
    }
    importer::detect(&data_dirs, &desktops)
}

/// 북마크 HTML, 크롬 Bookmarks, 파이어폭스 places.sqlite 중 무엇이든 가져온다.
#[tauri::command]
async fn import_file(
    app: AppHandle,
    path: String,
    source: Option<String>,
) -> Result<importer::ImportSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let items = importer::load(std::path::Path::new(&path))?;
        app.state::<Db>()
            .with(|c| importer::import_items(c, &items, source.as_deref().unwrap_or("file")))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 현재 필터에 맞는 북마크를 파일로 내보낸다. format 은 "html" 또는 "json". 내보낸 개수를 돌려준다.
#[tauri::command]
async fn export_file(
    app: AppHandle,
    path: String,
    format: String,
    filter: Option<ListFilter>,
) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (content, count) = app.state::<Db>().with(|c| {
            let items = bookmarks::list_all(c, &filter.unwrap_or_default())?;
            let content = match format.as_str() {
                "html" => exporter::to_netscape(&items),
                "json" => {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs() as i64);
                    exporter::to_json(c, &items, now)?
                }
                other => return Err(format!("지원하지 않는 형식입니다: {other}")),
            };
            Ok((content, items.len()))
        })?;
        std::fs::write(&path, content).map_err(|e| format!("파일을 쓸 수 없습니다: {e}"))?;
        Ok(count)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn extension_info(info: State<ExtensionInfo>) -> ExtensionInfo {
    info.inner().clone()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 이미 떠 있으면(트레이 포함) 새로 띄우지 않고 기존 창을 보여준다. 가장 먼저 등록해야 한다.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![tray::HIDDEN_ARG]),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let key = db::load_or_create_secret(db::DB_KEY)?;
            // 마지막에 쓰던 워크스페이스를 연다.
            let registry = workspaces::load(&dir);
            let active_dir = workspaces::dir(&dir, &registry.active);
            std::fs::create_dir_all(&active_dir)?;
            let conn = db::open(&active_dir.join("bookmarks.db"), &key)?;
            app.manage(Db(Mutex::new(conn)));
            app.manage(Workspaces(Mutex::new(registry)));
            app.manage(BackupCtx { dir: dir.clone(), key: Mutex::new(key) });

            let token = db::load_or_create_secret(db::EXTENSION_TOKEN)?;
            let error = extension_server::start(app.handle().clone(), token.clone()).err();
            app.manage(ExtensionInfo { port: extension_server::PORT, token, error });

            tray::setup(app)?;
            spawn_auto_backup(app.handle().clone());
            Ok(())
        })
        // 창을 닫으면 종료하지 않고 트레이로 숨긴다. 종료는 트레이 메뉴에서 한다.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            add_bookmark,
            list_bookmarks,
            count_bookmarks,
            update_bookmark,
            sidebar_counts,
            dashboard,
            open_bookmark,
            set_rating,
            delete_bookmark,
            delete_bookmarks,
            list_bookmark_ids,
            find_bookmark,
            set_thumbnail,
            get_thumbnail,
            remove_thumbnail,
            fetch_image,
            detect_browsers,
            import_file,
            export_file,
            extension_info,
            backup_status,
            set_backup_dir,
            backup_now,
            recovery_key,
            restore_backup,
            list_workspaces,
            create_workspace,
            rename_workspace,
            delete_workspace,
            switch_workspace,
            move_bookmarks,
            tray::set_tray_labels
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
