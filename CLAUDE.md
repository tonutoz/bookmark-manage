# Bookmark Manage

로컬 전용 북마크 관리 데스크톱 앱. Tauri 2 (Rust) + React 19 / TypeScript / Vite, 저장소는 SQLCipher로 암호화한 SQLite. 브라우저 확장(`extension/`)이 로컬 HTTP 서버로 앱에 저장한다. 주 대상은 Windows.

## 명령

```bash
npm install
npm run tauri dev        # 개발 실행 (Vite 1420 포트 + 앱)
npm run build            # tsc + vite build (프런트 타입 검사 겸용)
npm run tauri build      # 설치 파일 → src-tauri/target/release/bundle/{nsis,msi}
cd src-tauri && cargo test
cd src-tauri && cargo test -- --ignored   # 실제 브라우저 프로필 읽기, 5만 건 성능 측정
npm run tauri icon src-tauri/app-icon.svg # 아이콘 다시 만들기
```

Windows에서 Rust를 빌드할 때는 Strawberry Perl이 `PATH` 앞에 있어야 한다 (vendored OpenSSL). Git Bash 기준:

```bash
export PATH="/c/Strawberry/perl/bin:$USERPROFILE/.cargo/bin:$PATH"
```

링크 단계에서 `LNK1105`가 나면 파일 잠금이다. 다시 실행하면 된다.

## 구조

```
src/                 React 화면
src-tauri/src/       Rust 백엔드
src-tauri/migrations 스키마 (PRAGMA user_version 순서대로 적용)
extension/           WebExtension MV3 (번들러 없음, 순수 JS)
```

### 백엔드 (`src-tauri/src/`)

| 파일 | 역할 |
|---|---|
| `lib.rs` | Tauri 커맨드 전부, 상태(`Db`, `Workspaces`, `BackupCtx`), 자동 백업 스레드 |
| `db.rs` | 키체인에서 키 읽기/만들기, DB 열기, 마이그레이션 |
| `bookmarks.rs` | 북마크 CRUD, `ListFilter` 조회, 사이드바 집계, 대시보드 |
| `url_norm.rs` | 중복 판정용 주소 정규화 `normalize`, 사이트 이름 `site` |
| `thumbs.rs` | 썸네일 저장/조회/내려받기 |
| `importer.rs` / `exporter.rs` | 브라우저 형식 읽기·쓰기 |
| `extension_server.rs` | 확장용 HTTP 서버 (`127.0.0.1:43917`) |
| `backup.rs` | `.bmbackup` 스냅샷, 정리, 복원 |
| `workspaces.rs` | 워크스페이스 목록(`workspaces.json`)과 폴더 |
| `tray.rs` | 트레이 아이콘과 메뉴, 창 표시 |

### 데이터

- 앱 데이터 폴더: `%APPDATA%\com.personal.bookmark-manage`
- 워크스페이스마다 DB 파일이 따로다. `default`는 폴더 루트, 나머지는 `workspaces/<id>/`.
- 각 워크스페이스는 `bookmarks.db` + `thumbs.db`. 썸네일 DB는 `thumbs`라는 이름으로 ATTACH한다.
- 열려 있는 연결은 하나뿐이다. `Db(Mutex<Connection>)`를 워크스페이스 전환 때 바꿔 끼운다.
- 암호화 키는 OS 키체인에 있다 (서비스 `bookmark-manage`, 항목 `db-key`, `extension-token`). 모든 워크스페이스가 같은 키를 쓴다.
- 중복은 `bookmark.url_norm` UNIQUE로 막는다. 주소를 다루는 코드는 반드시 `url_norm::normalize`를 거친다.
- 검색은 FTS5 trigram(`bookmark_fts`). 3글자 미만 검색어는 LIKE로 제목·주소·메모·태그를 찾는다.

### 프런트 (`src/`)

- `App.tsx`가 페이지(목록/대시보드/설정), 필터, 선택, 삭제 대기 상태를 모두 쥔다. 라우터와 상태 라이브러리는 없다.
- 목록은 `@tanstack/react-virtual` 가상 스크롤 + 200개씩 페이지 로드. 늦게 온 응답은 세대 번호로 버린다.
- 삭제는 즉시 지우지 않는다. 화면에서 숨기고 되돌리기 토스트가 끝나면 실제로 지운다 (필터나 워크스페이스를 바꾸면 그 전에 확정).
- 썸네일은 등록할 때 `resizeImage.ts`가 줄여서 JPEG로 저장한다. 자르기는 CSS(`object-fit: cover`, 위쪽 기준)로만 한다.
- 스타일은 `App.css` 한 파일, CSS 변수로 색을 정의한다. 다크 모드는 `<html data-theme>`로 바꾼다 (`theme.ts`, 깜빡임 방지용 인라인 스크립트는 `index.html`).

## 규칙

### 다국어

- **한국어 문구가 곧 번역 키다.** 화면에 보이는 글자는 `t("한국어 문구")`로 감싸고, `src/locales.ts`에 `en`, `ja`를 추가한다. 번역이 없으면 한국어가 그대로 보인다.
- 자리 표시는 `{0}`, 영어 단복수는 `{0|bookmark|bookmarks}`.
- 백엔드 오류 문구도 한국어다. 화면에서는 `errorText()`로 번역해 보여준다. 새 오류 문구를 만들면 `locales.ts`에도 넣는다.
- 반복문 변수 이름으로 `t`를 쓰지 않는다. 번역 함수를 가린다.
- 언어는 로드할 때 정해진다. 언어를 바꾸면 페이지를 새로 읽는다.
- 확장은 번역표가 따로다 (`extension/i18n.js`). 팝업 문구를 바꾸면 여기도 고친다.
- 트레이 메뉴 글자는 화면이 `set_tray_labels`로 보내 준다.

### 스키마 변경

기존 마이그레이션 파일은 고치지 않는다. `src-tauri/migrations/00N_*.sql`을 새로 만들고 `db.rs`의 `MIGRATIONS`에 추가한다. 내보내기 JSON과 백업 형식에 영향이 있는지 함께 본다.

### 커맨드 추가

`lib.rs`에 `#[tauri::command]`를 만들고 `invoke_handler` 목록에 등록한다. 인자와 반환 구조체는 `#[serde(rename_all = "camelCase")]`, 프런트 타입은 `src/types.ts`. 새 플러그인 권한은 `src-tauri/capabilities/default.json`에 넣는다.

### CSS

`App.css`가 한 파일이라 클래스 이름이 전역이다. 새 클래스를 만들기 전에 같은 이름이 있는지 검색한다. (`.workspace`가 겹쳐서 본문 폭이 줄어든 적이 있다.)

### 확장 서버

`extension_server.rs`의 `guard()`가 Host 헤더, 출처(확장 프로그램만), Bearer 토큰을 검사한다. 경로를 추가해도 이 검사를 건너뛰지 않는다. 바인딩은 `127.0.0.1` 고정이다.

### 주석과 문구

주석과 사용자에게 보이는 문구는 한국어로 쓴다.

## 주의할 점

- 개발 빌드와 설치된 앱은 식별자가 같아 **같은 데이터 폴더, 같은 키, 같은 포트**를 쓴다. 단일 인스턴스라 동시에 뜨지 않는다. 개발 중에 만지는 데이터가 실사용 데이터다.
- 개발 빌드에서 트레이의 "윈도우 시작 시 실행"을 켜지 않는다. `target/debug` 경로가 등록된다.
- 창은 `visible: false`로 시작하고 `tray::setup`이 띄운다. `--hidden` 인자가 있으면 트레이로만 뜬다.
- 실제 DB를 직접 열어 고치거나 키체인에서 키를 꺼내지 않는다. 데이터를 바꿔야 하면 앱 기능으로 만든다.
- Tauri 창은 자동화 도구로 조작할 수 없다. 화면을 확인하려면 `@tauri-apps/api/mocks`의 `mockIPC`로 임시 페이지를 만들어 Vite 개발 서버에서 띄우고, 끝나면 지운다.
- 백엔드 테스트는 `db::open_in_memory()`를 쓴다. 키체인과 실제 파일을 건드리지 않는다.
