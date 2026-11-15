// 크롬은 서비스 워커로 이 파일만 읽으므로 api.js 를 직접 불러온다.
// 파이어폭스는 manifest 의 background.scripts 로 api.js 가 먼저 로드된다.
if (typeof importScripts === "function") importScripts("api.js");

// 앱이 꺼져 있을 때 보관해 둔 북마크를 1분마다 다시 보내 본다. 보관분이 없으면 요청하지 않는다.
function schedule() {
  chrome.alarms.create("flush", { periodInMinutes: 1 });
}
chrome.runtime.onInstalled.addListener(schedule);
chrome.runtime.onStartup.addListener(schedule);
chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "flush") api.flush();
});
