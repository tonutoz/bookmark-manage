const $ = (id) => document.getElementById(id);
const STAR = "8,1.5 10,6 14.8,6.4 11.1,9.6 12.3,14.3 8,11.7 3.7,14.3 4.9,9.6 1.2,6.4 6,6";
const CONNECTION = {
  checking: t("확인 중"),
  ok: t("연결됨"),
  off: t("앱 꺼짐"),
  setup: t("토큰 필요"),
  error: t("토큰 오류"),
};
let rating = null;
// 이미 저장된 페이지를 고치는 중이면 그 북마크의 id
let editingId = null;

/** kind: "" (안내) | "ok" | "error" */
function setStatus(text, kind = "") {
  $("status").textContent = text;
  $("status").dataset.kind = kind;
}

function setConnection(state) {
  $("connection").dataset.state = state;
  $("connection-text").textContent = CONNECTION[state];
}

function showSettings(open) {
  $("settings").hidden = !open;
  $("toggle-settings").setAttribute("aria-expanded", String(open));
  if (open) $("token").focus();
}

function renderStars() {
  $("stars").replaceChildren(
    ...[1, 2, 3, 4, 5].map((n) => {
      const star = document.createElement("button");
      star.type = "button";
      star.setAttribute("aria-label", t("별점 {0}점", n));
      star.setAttribute("aria-pressed", String(rating === n));
      star.innerHTML = `<svg width="22" height="22" viewBox="0 0 16 16" aria-hidden="true"><polygon points="${STAR}" fill="var(${n <= (rating ?? 0) ? "--star" : "--star-off"})"></polygon></svg>`;
      // 같은 별을 다시 누르면 평가를 지운다.
      star.onclick = () => {
        rating = rating === n ? null : n;
        renderStars();
      };
      return star;
    }),
  );
}

function hostOf(url) {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return "";
  }
}

/** 현재 탭의 파비콘을 보여주고, 없거나 못 읽으면 사이트 머리글자를 쓴다. */
function renderPage(tab) {
  const host = hostOf(tab?.url ?? "");
  $("page-title").textContent = tab?.title || host || t("현재 페이지");
  $("page-host").textContent = host;
  const box = $("favicon");
  const initial = (host || "?").charAt(0).toUpperCase();
  box.textContent = initial;
  if (/^(https?:|data:image\/)/.test(tab?.favIconUrl ?? "")) {
    const img = document.createElement("img");
    img.alt = "";
    img.onerror = () => (box.textContent = initial);
    img.onload = () => box.replaceChildren(img);
    img.src = tab.favIconUrl;
  }
}

/** 저장된 내용을 폼에 채우고 수정 모드로 바꾼다. */
function enterEdit(bookmark) {
  editingId = bookmark.id;
  $("title").value = bookmark.title;
  $("tags").value = bookmark.tags.map((t) => `#${t}`).join(" ");
  $("note").value = bookmark.note;
  rating = bookmark.rating;
  renderStars();
  $("duplicate").hidden = false;
  $("save").textContent = t("수정 저장");
}

/** 수정 모드를 풀고 새로 저장하는 상태로 돌아간다. */
function leaveEdit() {
  editingId = null;
  $("duplicate").hidden = true;
  $("save").textContent = t("저장");
}

/** 지금 페이지가 고른 워크스페이스에 이미 저장돼 있으면 수정 모드로 연다. */
async function loadExisting() {
  const bookmark = await api.lookup($("url").value);
  if (bookmark) enterEdit(bookmark);
  else leaveEdit();
}

/** 저장 위치 선택란을 채운다. 워크스페이스가 하나뿐이면 보여주지 않는다. */
async function loadWorkspaces() {
  const { active, items } = await api.workspaces();
  const stored = await api.workspace();
  // 고른 적이 없거나 고른 곳이 지워졌으면 앱에서 열려 있는 곳을 따른다.
  const chosen = items.some((w) => w.id === stored) ? stored : active;
  if (chosen !== stored) await chrome.storage.local.set({ workspace: chosen });
  $("workspace").replaceChildren(
    ...items.map((w) => {
      const option = document.createElement("option");
      option.value = w.id;
      // 처음부터 있는 워크스페이스는 이름을 바꾸지 않았다면 화면 언어로 보여준다.
      option.textContent = w.id === "default" ? t(w.name) : w.name;
      return option;
    }),
  );
  $("workspace").value = chosen;
  $("workspace-field").hidden = items.length < 2;
}

/** 앱 연결 상태를 확인하고, 보관해 둔 북마크가 있으면 보낸다. */
async function checkConnection() {
  if (!(await api.token())) {
    setConnection("setup");
    showSettings(true);
    setStatus(t("먼저 앱과 연결하세요."));
    return;
  }
  try {
    await api.ping();
    setConnection("ok");
    const left = await api.flush();
    if (left > 0) setStatus(t("보관 중인 북마크 {0}개를 아직 보내지 못했습니다.", left));
    await loadWorkspaces();
    await loadExisting();
  } catch (error) {
    if (error.status) {
      setConnection("error");
      showSettings(true);
      setStatus(t(error.message), "error");
    } else {
      setConnection("off");
      const waiting = (await api.pending()).length;
      setStatus(
        waiting > 0
          ? t("앱이 꺼져 있습니다. 보관 중인 북마크 {0}개는 앱을 켜면 전송됩니다.", waiting)
          : t("앱이 꺼져 있습니다. 지금 저장하면 보관했다가 앱을 켤 때 전송합니다."),
      );
    }
  }
}

async function init() {
  localizeDom();
  renderStars();
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  $("title").value = tab?.title ?? "";
  $("url").value = tab?.url ?? "";
  renderPage(tab);
  $("token").value = await api.token();
  await checkConnection();
}

// 저장 위치를 바꾸면 그 워크스페이스에 이미 저장된 페이지인지 다시 확인한다.
$("workspace").addEventListener("change", async () => {
  await chrome.storage.local.set({ workspace: $("workspace").value });
  setStatus("");
  try {
    await loadExisting();
  } catch (error) {
    setStatus(t(error.message), "error");
  }
});

$("toggle-settings").addEventListener("click", () => showSettings($("settings").hidden));

$("save-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  $("save").disabled = true;
  try {
    const fields = {
      url: $("url").value,
      title: $("title").value,
      note: $("note").value,
      rating,
      tags: $("tags").value.split(/[\s,]+/).map((t) => t.replace(/^#+/, "")).filter(Boolean),
    };
    if (editingId !== null) {
      await api.update({ id: editingId, ...fields });
      setConnection("ok");
      setStatus(t("수정했습니다."), "ok");
      $("save").textContent = t("수정됨");
      return;
    }
    const result = await api.save(fields);
    if (result.sent && result.duplicate) {
      // 팝업을 연 뒤에 다른 곳에서 먼저 저장된 경우
      enterEdit(result.bookmark);
      setConnection("ok");
      setStatus("");
      return;
    }
    // 방금 저장한 것을 이어서 고치면 새로 만들지 않고 수정한다.
    if (result.sent) editingId = result.bookmark.id;
    if (!result.sent) {
      setConnection("off");
      setStatus(t("앱이 꺼져 있어 보관했습니다. 앱을 켜면 전송됩니다."), "ok");
    } else {
      setConnection("ok");
      setStatus(result.duplicate ? t("이미 저장된 주소입니다.") : t("저장했습니다."), "ok");
    }
    $("save").textContent = result.sent && result.duplicate ? t("저장") : t("저장됨");
  } catch (error) {
    setStatus(t(error.message), "error");
    if (error.status === 401) {
      setConnection("error");
      showSettings(true);
    }
  } finally {
    $("save").disabled = false;
  }
});

// 내용을 고치면 다시 저장할 수 있다는 걸 버튼으로 알린다.
$("save-form").addEventListener("input", () => ($("save").textContent = editingId === null ? t("저장") : t("수정 저장")));

$("connect").addEventListener("click", async () => {
  await chrome.storage.local.set({ token: $("token").value.trim() });
  try {
    await api.ping();
    setConnection("ok");
    showSettings(false);
    setStatus(t("앱과 연결됐습니다."), "ok");
    await api.flush();
    await loadWorkspaces();
    await loadExisting();
  } catch (error) {
    setConnection(error.status ? "error" : "off");
    setStatus(error.status ? t(error.message) : t("앱에 연결할 수 없습니다. 앱이 켜져 있는지 확인하세요."), "error");
  }
});

init();
