// 확장 팝업의 다국어. 앱(src/i18n.ts)과 같은 방식이다: 한국어 문구가 번역표의 열쇠이고,
// 번역이 없는 문구는 한국어로 보인다. 언어는 브라우저의 언어를 따른다.
const TRANSLATIONS = {
  "확인 중": { en: "Checking", ja: "確認中" },
  "연결됨": { en: "Connected", ja: "接続済み" },
  "앱 꺼짐": { en: "App closed", ja: "アプリ停止中" },
  "토큰 필요": { en: "Token needed", ja: "トークンが必要" },
  "토큰 오류": { en: "Token error", ja: "トークンエラー" },
  "연결 설정": { en: "Connection settings", ja: "接続設定" },
  "앱과 연결": { en: "Connect to the app", ja: "アプリと接続" },
  "Bookmark Manage 앱에서 설정을 엽니다.": { en: "Open Settings in the Bookmark Manage app.", ja: "Bookmark Manage アプリで設定を開きます。" },
  "브라우저 확장 탭에서 연결 토큰을 복사합니다.": { en: "Copy the connection token from the Browser extension tab.", ja: "「ブラウザ拡張機能」タブで接続トークンをコピーします。" },
  "아래에 붙여넣고 연결 확인을 누릅니다.": { en: "Paste it below and choose Check connection.", ja: "下に貼り付けて「接続を確認」を押します。" },
  "연결 토큰": { en: "Connection token", ja: "接続トークン" },
  "연결 확인": { en: "Check connection", ja: "接続を確認" },
  "이미 저장된 페이지입니다.": { en: "This page is already saved.", ja: "このページは保存済みです。" },
  "저장된 내용을 불러왔습니다. 고친 뒤 수정 저장을 누르세요.": { en: "Its saved details are loaded. Edit them, then choose Save changes.", ja: "保存済みの内容を読み込みました。編集して「変更を保存」を押してください。" },
  "저장 위치": { en: "Save to", ja: "保存先" },
  "제목": { en: "Title", ja: "タイトル" },
  "주소": { en: "URL", ja: "URL" },
  "태그": { en: "Tags", ja: "タグ" },
  "개발 Rust 읽을거리": { en: "dev rust reading", ja: "開発 Rust 読み物" },
  "메모": { en: "Note", ja: "メモ" },
  "별점": { en: "Rating", ja: "評価" },
  "저장": { en: "Save", ja: "保存" },
  "저장됨": { en: "Saved", ja: "保存済み" },
  "수정 저장": { en: "Save changes", ja: "変更を保存" },
  "수정됨": { en: "Changes saved", ja: "変更済み" },
  "개인": { en: "Personal", ja: "個人" },
  "별점 {0}점": { en: "{0} stars", ja: "評価 {0}" },
  "현재 페이지": { en: "Current page", ja: "現在のページ" },
  "먼저 앱과 연결하세요.": { en: "Connect to the app first.", ja: "まずアプリと接続してください。" },
  "보관 중인 북마크 {0}개를 아직 보내지 못했습니다.": { en: "{0} held bookmarks haven't been sent yet.", ja: "保管中のブックマーク{0}件をまだ送信できていません。" },
  "앱이 꺼져 있습니다. 보관 중인 북마크 {0}개는 앱을 켜면 전송됩니다.": { en: "The app is closed. {0} held bookmarks will be sent when it starts.", ja: "アプリが閉じています。保管中のブックマーク{0}件は、アプリを起動すると送信されます。" },
  "앱이 꺼져 있습니다. 지금 저장하면 보관했다가 앱을 켤 때 전송합니다.": { en: "The app is closed. Save now and it'll be held and sent when the app starts.", ja: "アプリが閉じています。いま保存すると保管しておき、アプリの起動時に送信します。" },
  "수정했습니다.": { en: "Changes saved.", ja: "変更を保存しました。" },
  "앱이 꺼져 있어 보관했습니다. 앱을 켜면 전송됩니다.": { en: "The app is closed, so this is held. It'll be sent when the app starts.", ja: "アプリが閉じているため保管しました。アプリを起動すると送信されます。" },
  "이미 저장된 주소입니다.": { en: "This URL is already saved.", ja: "このURLは保存済みです。" },
  "저장했습니다.": { en: "Saved.", ja: "保存しました。" },
  "앱과 연결됐습니다.": { en: "Connected to the app.", ja: "アプリと接続しました。" },
  "앱에 연결할 수 없습니다. 앱이 켜져 있는지 확인하세요.": { en: "Can't reach the app. Check that it's running.", ja: "アプリに接続できません。アプリが起動しているか確認してください。" },
  // 앱이 돌려주는 오류 문구
  "토큰이 맞지 않습니다. 앱에서 토큰을 다시 복사해 넣어 주세요": { en: "The token doesn't match. Copy it from the app again and paste it in", ja: "トークンが一致しません。アプリでもう一度コピーして貼り付けてください" },
  "URL이 비어 있습니다": { en: "The URL is empty", ja: "URLが空です" },
  "같은 주소의 북마크가 이미 있습니다": { en: "A bookmark with this URL already exists", ja: "同じURLのブックマークがすでにあります" },
  "북마크를 찾을 수 없습니다": { en: "Bookmark not found", ja: "ブックマークが見つかりません" },
  "별점은 1~5 사이여야 합니다": { en: "A rating must be between 1 and 5", ja: "評価は1〜5で指定してください" },
  "허용되지 않은 호스트입니다": { en: "That host isn't allowed", ja: "許可されていないホストです" },
  "브라우저 확장에서만 사용할 수 있습니다": { en: "Only available to the browser extension", ja: "ブラウザ拡張機能からのみ利用できます" },
};

const LANG = (() => {
  const system = (navigator.language || "ko").toLowerCase();
  if (system.startsWith("ja")) return "ja";
  if (system.startsWith("ko")) return "ko";
  return "en";
})();

/** 한국어 문구를 지금 언어로 바꾸고 {0}, {1} 자리를 채운다. */
function t(key, ...args) {
  const text = (LANG !== "ko" && TRANSLATIONS[key]?.[LANG]) || key;
  return text.replace(/\{(\d+)\}/g, (_, index) => String(args[Number(index)] ?? ""));
}

/** popup.html 에 적어 둔 한국어 글자와 속성을 지금 언어로 바꾼다. */
function localizeDom() {
  document.documentElement.lang = LANG;
  if (LANG === "ko") return;
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const text = node.nodeValue.trim().replace(/\s+/g, " ");
    if (text && TRANSLATIONS[text]) node.nodeValue = node.nodeValue.replace(node.nodeValue.trim(), t(text));
  }
  for (const element of document.querySelectorAll("[placeholder], [aria-label], [title]")) {
    for (const name of ["placeholder", "aria-label", "title"]) {
      const value = element.getAttribute(name);
      if (value && TRANSLATIONS[value]) element.setAttribute(name, t(value));
    }
  }
}
