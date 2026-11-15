//! 브라우저 확장이 북마크를 보내는 로컬 수신 서버.
//! 127.0.0.1 에서만 듣고, 앱이 발급한 토큰이 있어야 하며, 일반 웹페이지의 요청은 거부한다.

use std::io::Read;

use rusqlite::Connection;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tiny_http::{Header, Response, Server};

use crate::bookmarks::{self, BookmarkEdit, NewBookmark};

/// 확장 프로그램(extension/api.js)과 같은 값이어야 한다.
pub const PORT: u16 = 43917;
const MAX_BODY: u64 = 1024 * 1024;
/// 목록을 다시 읽으라고 화면에 알리는 이벤트.
pub const CHANGED_EVENT: &str = "bookmarks-changed";

/// 확장이 이미 저장된 북마크를 고칠 때 보내는 내용.
#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct Update {
    id: i64,
    rating: Option<i64>,
    #[serde(flatten)]
    edit: BookmarkEdit,
}

#[derive(Default)]
pub struct Req<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub host: Option<&'a str>,
    pub origin: Option<&'a str>,
    pub auth: Option<&'a str>,
    pub body: &'a str,
}

fn is_extension_origin(origin: &str) -> bool {
    origin.starts_with("chrome-extension://") || origin.starts_with("moz-extension://")
}

fn same(a: &str, b: &str) -> bool {
    // 길이가 같을 때 비교 시간이 내용에 좌우되지 않게 한다.
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// 요청 하나를 처리해 (상태 코드, JSON 본문) 을 돌려준다.
#[cfg(test)]
pub fn route(conn: &Connection, token: &str, req: &Req) -> (u16, Value) {
    guard(token, req).unwrap_or_else(|| dispatch(conn, req))
}

/// 믿을 수 있는 요청인지 확인한다. 거절하거나 바로 답할 것이 있으면 그 응답을 돌려준다.
pub fn guard(token: &str, req: &Req) -> Option<(u16, Value)> {
    let error = |status: u16, message: &str| Some((status, json!({ "error": message })));

    // 다른 이름으로 접속을 유도하는 공격(DNS 리바인딩) 차단
    let local = [format!("127.0.0.1:{PORT}"), format!("localhost:{PORT}")];
    if !req.host.is_some_and(|h| local.iter().any(|l| l == h)) {
        return error(403, "허용되지 않은 호스트입니다");
    }
    if req.origin.is_some_and(|o| !is_extension_origin(o)) {
        return error(403, "브라우저 확장에서만 사용할 수 있습니다");
    }
    if req.method == "OPTIONS" {
        return Some((200, json!({})));
    }
    let authorized = req
        .auth
        .and_then(|a| a.strip_prefix("Bearer "))
        .is_some_and(|given| same(given.trim(), token));
    if !authorized {
        return error(401, "토큰이 맞지 않습니다. 앱에서 토큰을 다시 복사해 넣어 주세요");
    }
    None
}

/// 확인을 통과한 요청을 주어진 워크스페이스의 DB 에서 처리한다.
pub fn dispatch(conn: &Connection, req: &Req) -> (u16, Value) {
    let error = |status: u16, message: &str| (status, json!({ "error": message }));

    match (req.method, req.path) {
        ("GET", "/ping") => (200, json!({ "ok": true, "app": "bookmark-manage" })),
        // 지금 보고 있는 페이지가 이미 저장돼 있는지. 있으면 확장이 수정 모드로 바뀐다.
        ("POST", "/lookup") => {
            let url = serde_json::from_str::<Value>(req.body).ok().and_then(|v| v["url"].as_str().map(str::to_string));
            match url.map(|url| bookmarks::find_by_url(conn, &url)) {
                Some(Ok(found)) => (200, json!({ "bookmark": found })),
                Some(Err(e)) => error(400, &e),
                None => error(400, "요청에 url 이 없습니다"),
            }
        }
        ("POST", "/update") => {
            let update: Update = match serde_json::from_str(req.body) {
                Ok(update) => update,
                Err(e) => return error(400, &format!("요청 형식이 잘못됐습니다: {e}")),
            };
            match bookmarks::update(conn, update.id, update.edit)
                .and_then(|_| bookmarks::set_rating(conn, update.id, update.rating))
            {
                Ok(bookmark) => (200, json!({ "updated": true, "bookmark": bookmark })),
                Err(e) => error(400, &e),
            }
        }
        ("POST", "/bookmarks") => {
            let mut new: NewBookmark = match serde_json::from_str(req.body) {
                Ok(new) => new,
                Err(e) => return error(400, &format!("요청 형식이 잘못됐습니다: {e}")),
            };
            new.source = Some("extension".into());
            match bookmarks::add(conn, new) {
                Ok(saved) => (200, json!(saved)),
                Err(e) => error(400, &e),
            }
        }
        _ => error(404, "없는 주소입니다"),
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid header")
}

fn handle(app: &AppHandle, token: &str, mut request: tiny_http::Request) {
    let find = |name: &'static str| {
        request.headers().iter().find(|h| h.field.equiv(name)).map(|h| h.value.as_str().to_string())
    };
    let (host, origin, auth) = (find("Host"), find("Origin"), find("Authorization"));
    let method = request.method().to_string();
    let path = request.url().split('?').next().unwrap_or("").to_string();
    let mut body = String::new();
    let _ = request.as_reader().take(MAX_BODY).read_to_string(&mut body);

    let req = Req {
        method: &method,
        path: &path,
        host: host.as_deref(),
        origin: origin.as_deref(),
        auth: auth.as_deref(),
        body: &body,
    };
    // 어느 워크스페이스의 DB 에서 처리할지는 앱 쪽(lib.rs)이 정한다.
    let (status, payload) = guard(token, &req).unwrap_or_else(|| crate::serve_extension(app, &req));

    if status == 200 && (payload["duplicate"] == false || payload["updated"] == true) {
        let _ = app.emit(CHANGED_EVENT, ());
    }

    let mut response = Response::from_string(payload.to_string())
        .with_status_code(status)
        .with_header(header("Content-Type", "application/json; charset=utf-8"));
    if let Some(origin) = origin.filter(|o| is_extension_origin(o)) {
        for (name, value) in [
            ("Access-Control-Allow-Origin", origin.as_str()),
            ("Access-Control-Allow-Headers", "authorization, content-type"),
            ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
            ("Access-Control-Allow-Private-Network", "true"),
            ("Vary", "Origin"),
        ] {
            response.add_header(header(name, value));
        }
    }
    let _ = request.respond(response);
}

/// 수신 서버를 백그라운드 스레드로 띄운다. 포트를 못 잡으면 이유를 돌려준다.
pub fn start(app: AppHandle, token: String) -> Result<(), String> {
    let server = Server::http(("127.0.0.1", PORT))
        .map_err(|e| format!("포트 {PORT} 을 열 수 없습니다: {e}"))?;
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            handle(&app, &token, request);
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "secret-token";
    const HOST: &str = "127.0.0.1:43917";

    fn req<'a>(method: &'a str, path: &'a str, body: &'a str) -> Req<'a> {
        Req {
            method,
            path,
            body,
            host: Some(HOST),
            origin: Some("chrome-extension://abcdefgh"),
            auth: Some("Bearer secret-token"),
        }
    }

    #[test]
    fn saves_and_reports_duplicates() {
        let conn = crate::db::open_in_memory();
        let body = r##"{"url":"https://a.example/","title":"A","tags":["#dev"],"rating":5}"##;
        let (status, first) = route(&conn, TOKEN, &req("POST", "/bookmarks", body));
        assert_eq!(status, 200);
        assert_eq!(first["duplicate"], false);
        assert_eq!(first["bookmark"]["tags"], json!(["dev"]));
        assert_eq!(first["bookmark"]["rating"], 5);

        let (status, second) = route(&conn, TOKEN, &req("POST", "/bookmarks", body));
        assert_eq!((status, &second["duplicate"]), (200, &json!(true)));

        assert_eq!(route(&conn, TOKEN, &req("GET", "/ping", "")).0, 200);
        assert_eq!(route(&conn, TOKEN, &req("POST", "/bookmarks", "not json")).0, 400);
        assert_eq!(route(&conn, TOKEN, &req("POST", "/bookmarks", r#"{"url":""}"#)).0, 400);
        assert_eq!(route(&conn, TOKEN, &req("GET", "/nope", "")).0, 404);
    }

    #[test]
    fn lookup_then_update_existing() {
        let conn = crate::db::open_in_memory();
        let body = r##"{"url":"https://a.example/post/","title":"A","tags":["dev"]}"##;
        let saved = route(&conn, TOKEN, &req("POST", "/bookmarks", body)).1;
        let id = saved["bookmark"]["id"].as_i64().unwrap();

        // 같은 페이지를 조금 다른 주소로 열어도 찾는다.
        let (status, found) = route(&conn, TOKEN, &req("POST", "/lookup", r#"{"url":"http://a.example/post?utm_source=x"}"#));
        assert_eq!((status, found["bookmark"]["id"].as_i64()), (200, Some(id)));
        let (status, none) = route(&conn, TOKEN, &req("POST", "/lookup", r#"{"url":"https://other.example/"}"#));
        assert_eq!((status, none["bookmark"].is_null()), (200, true));
        assert_eq!(route(&conn, TOKEN, &req("POST", "/lookup", "{}")).0, 400);

        let update = format!(r##"{{"id":{id},"url":"https://a.example/post/","title":"고친 제목","note":"메모","tags":["dev","#읽을거리"],"rating":4}}"##);
        let (status, updated) = route(&conn, TOKEN, &req("POST", "/update", &update));
        assert_eq!(status, 200);
        assert_eq!(updated["updated"], true);
        assert_eq!(updated["bookmark"]["title"], "고친 제목");
        assert_eq!(updated["bookmark"]["tags"], json!(["dev", "읽을거리"]));
        assert_eq!(updated["bookmark"]["rating"], 4);

        assert_eq!(route(&conn, TOKEN, &req("POST", "/update", r#"{"id":9999,"url":"https://x.example"}"#)).0, 400);
        // 조회와 수정도 토큰이 있어야 한다.
        assert_eq!(route(&conn, TOKEN, &Req { auth: None, ..req("POST", "/lookup", body) }).0, 401);
        assert_eq!(route(&conn, TOKEN, &Req { auth: None, ..req("POST", "/update", &update) }).0, 401);
    }

    #[test]
    fn rejects_untrusted_callers() {
        let conn = crate::db::open_in_memory();
        let ping = || req("GET", "/ping", "");

        assert_eq!(route(&conn, TOKEN, &Req { auth: None, ..ping() }).0, 401);
        assert_eq!(route(&conn, TOKEN, &Req { auth: Some("Bearer wrong-token!"), ..ping() }).0, 401);
        assert_eq!(route(&conn, TOKEN, &Req { auth: Some("secret-token"), ..ping() }).0, 401);
        // 일반 웹페이지에서 온 요청
        assert_eq!(route(&conn, TOKEN, &Req { origin: Some("https://evil.example"), ..ping() }).0, 403);
        // 다른 호스트 이름으로 들어온 요청
        assert_eq!(route(&conn, TOKEN, &Req { host: Some("evil.example:43917"), ..ping() }).0, 403);
        assert_eq!(route(&conn, TOKEN, &Req { host: None, ..ping() }).0, 403);
        // 사전 요청(preflight)은 토큰 없이 통과하지만 아무것도 하지 않는다
        assert_eq!(route(&conn, TOKEN, &Req { method: "OPTIONS", auth: None, ..ping() }).0, 200);
        // Origin 이 없는 로컬 도구(curl 등)는 토큰이 맞으면 허용
        assert_eq!(route(&conn, TOKEN, &Req { origin: None, ..ping() }).0, 200);
    }
}
