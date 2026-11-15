# Bookmark Manage

내 PC에만 저장하는 개인용 북마크 관리 프로그램입니다. 서버나 계정 없이 동작하고, 데이터는 암호화된 로컬 데이터베이스에 들어갑니다. 브라우저 확장으로 보고 있는 페이지를 바로 저장할 수 있습니다.

Windows를 주 대상으로 만들었습니다. macOS에서도 빌드할 수 있는 구조지만 확인해 보지는 않았습니다.

## 기능

- **북마크**: 제목, 주소, 메모, 별점(5점), 태그, 썸네일
- **썸네일**: 파일, 이미지 주소, 클립보드 붙여넣기로 직접 등록
- **중복 방지**: 주소를 정규화해서 비교합니다. `http`/`https`, 끝의 `/`, `#` 뒤, `utm_*` 같은 추적 매개변수가 달라도 같은 주소로 봅니다. 이미 있는 주소를 넣으면 기존 북마크 수정으로 안내합니다.
- **검색과 필터**: 제목·주소·메모·태그 검색, 태그, 별점, 사이트(도메인)별 자동 묶음, 태그 없음, 한 번도 안 연 것, 사이트에 하나뿐인 것
- **열람 횟수**: 앱에서 연 횟수와 마지막으로 연 시각을 기록하고, 자주 여는 순으로 정렬
- **대시보드**: 전체 현황, 정리할 것(태그 없음, 별점 없음, 오래 안 연 것), 별점 분포, 자주 여는 북마크, 월별 추가량
- **워크스페이스**: 업무와 개인처럼 완전히 분리된 공간. 워크스페이스마다 데이터베이스 파일이 따로입니다.
- **가져오기**: Chrome, Edge, Firefox, Tor Browser 등의 프로필을 자동으로 찾아 가져오거나, 북마크 HTML 파일 / Chromium `Bookmarks` 파일 / Firefox `places.sqlite` / 이 앱의 JSON 내보내기 파일을 직접 지정
- **내보내기**: 모든 브라우저가 읽는 북마크 HTML, 또는 별점·태그·메모까지 담는 JSON
- **삭제**: 선택 삭제, 필터 결과 일괄 삭제(되돌리기 가능). 사이트 전체 삭제는 도메인을 직접 입력해야 진행됩니다.
- **백업**: 지정한 폴더에 `.bmbackup` 파일을 주기적으로 저장하고 오래된 것은 정리합니다. Google Drive나 OneDrive 동기화 폴더를 지정하면 클라우드 백업이 됩니다.
- **트레이 상주**: 창을 닫아도 트레이에 남아 확장 프로그램의 저장 요청을 받습니다.
- **화면**: 라이트 / 다크 / 시스템 설정 따름, 한국어 / 영어 / 일본어

링크가 죽었는지 검사하는 기능과 썸네일 자동 수집은 아직 없습니다.

## 설치

[Releases](../../releases)에서 `Bookmark Manage_x.y.z_x64-setup.exe`를 받아 실행합니다.

코드 서명을 하지 않았기 때문에 처음 실행할 때 "Windows의 PC 보호" 화면이 나올 수 있습니다. "추가 정보 → 실행"을 누르면 됩니다.

## 브라우저 확장

`extension/` 폴더가 확장 프로그램입니다. 스토어에는 올리지 않았으므로 직접 불러옵니다.

**Chrome, Edge**

1. `chrome://extensions`를 열고 "개발자 모드"를 켭니다.
2. "압축해제된 확장 프로그램을 로드합니다"를 눌러 `extension` 폴더를 선택합니다.

**Firefox**

1. `about:debugging#/runtime/this-firefox`를 엽니다.
2. "임시 부가 기능 로드"를 눌러 `extension/manifest.json`을 선택합니다. 브라우저를 다시 시작하면 사라집니다.

**앱과 연결**

1. 앱의 설정 → 브라우저 확장 탭에서 연결 토큰을 복사합니다.
2. 확장 팝업에 붙여넣고 "연결 확인"을 누릅니다.

앱이 꺼져 있을 때 저장한 북마크는 브라우저에 보관했다가 앱이 켜지면 전송합니다.

태그는 띄어쓰기나 쉼표로 구분합니다. `#`은 붙여도 되고 안 붙여도 됩니다.

## 데이터와 보안

| 항목 | 내용 |
|---|---|
| 저장 위치 | `%APPDATA%\com.personal.bookmark-manage` |
| 암호화 | SQLCipher. 북마크와 썸네일 데이터베이스 모두 암호화됩니다. |
| 암호화 키 | 처음 실행할 때 무작위로 만들어 OS 키체인(Windows 자격 증명 관리자)에 저장합니다. |
| 확장 프로그램 통신 | `127.0.0.1:43917`에서만 받습니다. 토큰이 맞고, 브라우저 확장에서 온 요청만 처리합니다. |
| 외부 전송 | 없습니다. 이미지 주소로 썸네일을 등록할 때만 그 주소에 접속합니다. |

**복구 키를 따로 보관하세요.** 암호화 키는 이 PC의 키체인에만 있습니다. Windows를 다시 설치하거나 다른 PC로 옮기면 백업 파일이 있어도 키가 없어 열 수 없습니다. 설정 → 백업 탭에서 복구 키를 확인해 안전한 곳에 적어 두면, 백업을 복원할 때 그 키를 입력해 되살릴 수 있습니다.

## 직접 빌드하기

### 준비물

- [Node.js](https://nodejs.org/) 22 이상
- [Rust](https://rustup.rs/) (stable)
- Windows: [Strawberry Perl](https://strawberryperl.com/). SQLCipher가 쓰는 OpenSSL을 소스에서 빌드하는 데 필요합니다. Git에 딸린 Perl로는 빌드가 실패하므로 `C:\Strawberry\perl\bin`이 `PATH`에서 앞에 와야 합니다.
- Windows: Microsoft C++ Build Tools, WebView2 (Windows 11에는 기본 포함)

### 명령

```bash
npm install
```

```bash
npm run tauri dev
```

```bash
npm run tauri build
```

설치 파일은 `src-tauri/target/release/bundle/` 아래 `nsis`, `msi` 폴더에 만들어집니다. 처음 빌드는 OpenSSL과 SQLCipher를 컴파일하느라 10분쯤 걸립니다.

개발 빌드와 설치된 앱은 같은 데이터 폴더, 같은 키, 같은 포트를 씁니다. 둘을 동시에 띄울 수 없습니다.

### 테스트

```bash
cd src-tauri && cargo test
```

## 기술 구성

- [Tauri 2](https://tauri.app/): Rust 백엔드 + 시스템 WebView
- React 19, TypeScript, Vite
- SQLite + SQLCipher ([rusqlite](https://github.com/rusqlite/rusqlite)), FTS5 trigram 검색
- WebExtension Manifest V3 (Chrome, Firefox 공용)

구조와 개발 규칙은 [CLAUDE.md](CLAUDE.md)에 정리했습니다.

## 라이선스

[MIT](LICENSE). 자유롭게 쓰고, 고치고, 다시 배포할 수 있습니다.

함께 들어 있는 글꼴(Manrope, Space Grotesk, JetBrains Mono)은 각각 SIL Open Font License 1.1을 따릅니다.
